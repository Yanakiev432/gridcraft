// In-process native Windows OPC API adapter; no child processes.
// Signature validation checks OPC XMLDSig integrity and rejects unsigned content;
// certificate trust, revocation, and eIDAS QES need separate acceptance. MIT/Apache-2.0.
#define WIN32_LEAN_AND_MEAN
#include <Windows.h>
#include <msopc.h>
#include <urlmon.h>
#include <wincrypt.h>
#include <ncrypt.h>
#include <wrl/client.h>
#include <string>
#include <vector>
#include <cstdio>

using Microsoft::WRL::ComPtr;

namespace {
// QA-only, opt-in stage breadcrumbs. Do not log document paths, subjects, IDs, keys or PINs.
void craft_trace_stage(const char* stage, long hr = 0) noexcept {
  wchar_t folder[MAX_PATH] = {};
  const DWORD n = GetEnvironmentVariableW(L"CRAFT_OPC_TRACE_DIR", folder, MAX_PATH);
  if(n == 0 || n >= MAX_PATH) return;
  const DWORD attributes = GetFileAttributesW(folder);
  if(attributes == INVALID_FILE_ATTRIBUTES || (attributes & FILE_ATTRIBUTE_DIRECTORY) == 0) return;
  wchar_t path[MAX_PATH] = {};
  if(_snwprintf_s(path, MAX_PATH, _TRUNCATE, L"%ls\\opc-stage-%lu.log",
                  folder, static_cast<unsigned long>(GetCurrentProcessId())) < 0) return;
  HANDLE file = CreateFileW(path, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
                             OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
  if(file == INVALID_HANDLE_VALUE) return;
  SYSTEMTIME when = {};
  GetSystemTime(&when);
  char line[240] = {};
  const int count = _snprintf_s(line, sizeof(line), _TRUNCATE,
      "%04u-%02u-%02uT%02u:%02u:%02u.%03uZ tid=%lu stage=%s hr=0x%08lX\r\n",
      when.wYear,when.wMonth,when.wDay,when.wHour,when.wMinute,when.wSecond,when.wMilliseconds,
      static_cast<unsigned long>(GetCurrentThreadId()),stage,
      static_cast<unsigned long>(hr));
  if(count > 0) { DWORD written = 0; WriteFile(file, line, static_cast<DWORD>(count), &written, nullptr); }
  CloseHandle(file);
}
// Find an owned, visible top-level app window without reading titles or content.
struct CraftWindowCandidate {
  DWORD pid = 0;
  HWND result = nullptr;
};
BOOL CALLBACK craft_find_window(HWND window, LPARAM opaque) {
  auto* request = reinterpret_cast<CraftWindowCandidate*>(opaque);
  DWORD pid = 0;
  GetWindowThreadProcessId(window, &pid);
  if(pid != request->pid || !IsWindowVisible(window) || GetWindow(window, GW_OWNER)) return TRUE;
  request->result = window;
  return FALSE;
}
HWND craft_find_owner_window() {
  DWORD pid = GetCurrentProcessId();
  HWND foreground = GetForegroundWindow();
  DWORD foregroundPid = 0;
  if(foreground) GetWindowThreadProcessId(foreground, &foregroundPid);
  if(foreground && foregroundPid == pid && IsWindowVisible(foreground)) return foreground;
  CraftWindowCandidate request;
  request.pid = pid;
  EnumWindows(craft_find_window, reinterpret_cast<LPARAM>(&request));
  return request.result;
}
bool craft_pin_window_opt_in() {
  wchar_t option[4] = {};
  return GetEnvironmentVariableW(L"CRAFT_OPC_BIND_PIN_WINDOW", option, 4) == 1 &&
         option[0] == L'1';
}
struct ComInit {
  HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
  ~ComInit() { if (SUCCEEDED(hr)) CoUninitialize(); }
};
bool absolute_path(const wchar_t* p) {
  if (!p || !*p) return false;
  // C:\... or \\server\share\...; deny relative paths and drive-relative forms.
  const size_t n = wcslen(p);
  return (n >= 4 && p[1] == L':' && p[2] == L'\\') ||
         (n >= 5 && p[0] == L'\\' && p[1] == L'\\');
}
int hexval(wchar_t x) {
  if(x>=L'0'&&x<=L'9') return x-L'0';
  if(x>=L'a'&&x<=L'f') return x-L'a'+10;
  if(x>=L'A'&&x<=L'F') return x-L'A'+10;
  return -1;
}
bool parse_sha1(const wchar_t* text, BYTE out[20]) {
  if(!text || wcslen(text)!=40) return false;
  for(int i=0;i<20;++i) {
    int a=hexval(text[2*i]),b=hexval(text[2*i+1]);
    if(a<0||b<0) return false;
    out[i]=static_cast<BYTE>((a<<4)|b);
  }
  return true;
}
HRESULT make_factory(ComPtr<IOpcFactory>& factory) {
  return CoCreateInstance(__uuidof(OpcFactory),nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&factory));
}
HRESULT load_package(IOpcFactory* factory,const wchar_t* path,ComPtr<IOpcPackage>& package) {
  ComPtr<IStream> stream;
  HRESULT hr=factory->CreateStreamOnFile(path,OPC_STREAM_IO_READ,nullptr,FILE_ATTRIBUTE_NORMAL,&stream);
  if(FAILED(hr)) return hr;
  return factory->ReadPackageFromStream(stream.Get(),OPC_VALIDATE_ON_LOAD,&package);
}
HRESULT append_relationships(IOpcSignatureRelationshipReferenceSet* refs, IOpcUri* source, IOpcRelationshipSet* set) {
  ComPtr<IOpcRelationshipEnumerator> en;
  HRESULT hr=set->GetEnumerator(&en);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcRelationshipSelectorSet> selectors;
  hr=refs->CreateRelationshipSelectorSet(&selectors);
  if(FAILED(hr)) return hr;
  int count=0;
  BOOL more=FALSE;
  while(SUCCEEDED(hr=en->MoveNext(&more)) && more) {
    ComPtr<IOpcRelationship> item;
    hr=en->GetCurrent(&item);
    if(FAILED(hr)) return hr;
    LPWSTR id=nullptr;
    hr=item->GetId(&id);
    if(FAILED(hr)) return hr;
    ComPtr<IOpcRelationshipSelector> selector;
    hr=selectors->Create(OPC_RELATIONSHIP_SELECT_BY_ID,id,&selector);
    CoTaskMemFree(id);
    if(FAILED(hr)) return hr;
    ++count;
  }
  if(FAILED(hr)||!count) return hr;
  ComPtr<IOpcSignatureRelationshipReference> signedRelation;
  return refs->Create(source,nullptr,OPC_RELATIONSHIP_SIGN_USING_SELECTORS,
                      selectors.Get(),OPC_CANONICALIZATION_C14N,&signedRelation);
}
bool signature_metadata_part(IOpcPart* part);
HRESULT crypto_verify(IOpcPackage* package, IOpcDigitalSignatureManager* manager, int* checked);
// Office-specific markup is distinct from OPC's package manifest. It must be
// placed INSIDE SignedInfo via a digest reference: a loose XML Object is unsafe.
// This is an invisible Office signature, not a native Office signature line;
// the visible Word/Excel image is an ordinary OOXML drawing.
HRESULT add_office_signature_object(IOpcSigningOptions* options) {
  static const UINT8 xml[] =
    "<Object Id=\"idOfficeObject\" xmlns=\"http://www.w3.org/2000/09/xmldsig#\">"
      "<SignatureProperties>"
        "<SignatureProperty Id=\"idOfficeV1Details\" Target=\"#idPackageSignature\">"
          "<SignatureInfoV1 xmlns=\"http://schemas.microsoft.com/office/2006/digsig\">"
            "<SetupID/>"
            "<SignatureText/>"
            "<SignatureImage/>"
            "<SignatureComments/>"
            "<WindowsVersion>10.0</WindowsVersion>"
            "<OfficeVersion>0.1</OfficeVersion>"
            "<ApplicationVersion>0.1</ApplicationVersion>"
            "<Monitors>1</Monitors>"
            "<HorizontalResolution>96</HorizontalResolution>"
            "<VerticalResolution>96</VerticalResolution>"
            "<ColorDepth>32</ColorDepth>"
            "<SignatureProviderId>{00000000-0000-0000-0000-000000000000}</SignatureProviderId>"
            "<SignatureProviderUrl/>"
            "<SignatureProviderDetails>0</SignatureProviderDetails>"
            "<SignatureType>1</SignatureType>"
          "</SignatureInfoV1>"
        "</SignatureProperty>"
      "</SignatureProperties>"
    "</Object>";
  ComPtr<IOpcSignatureCustomObjectSet> objects;
  HRESULT hr=options->GetCustomObjectSet(&objects);
  if(FAILED(hr)) return hr;
  hr=objects->Create(xml,sizeof(xml)-1,nullptr);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcSignatureReferenceSet> refs;
  hr=options->GetCustomReferenceSet(&refs);
  if(FAILED(hr)) return hr;
  ComPtr<IUri> officeRef;
  hr=CreateUri(L"#idOfficeObject",Uri_CREATE_ALLOW_RELATIVE,0,&officeRef);
  if(FAILED(hr)) return hr;
  return refs->Create(officeRef.Get(),nullptr,
    L"http://www.w3.org/2000/09/xmldsig#Object",nullptr,
    OPC_CANONICALIZATION_C14N,nullptr);
}

HRESULT sign_parts(IOpcFactory* factory,IOpcPackage* package,IOpcSigningOptions* options) {
  ComPtr<IOpcSignaturePartReferenceSet> partRefs;
  ComPtr<IOpcSignatureRelationshipReferenceSet> relRefs;
  HRESULT hr=options->GetSignaturePartReferenceSet(&partRefs);
  if(FAILED(hr)) return hr;
  hr=options->GetSignatureRelationshipReferenceSet(&relRefs);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcPartSet> parts;
  hr=package->GetPartSet(&parts);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcPartEnumerator> en;
  hr=parts->GetEnumerator(&en);
  if(FAILED(hr)) return hr;
  BOOL more=FALSE;
  int signedParts=0;
  while(SUCCEEDED(hr=en->MoveNext(&more)) && more) {
    ComPtr<IOpcPart> part;
    hr=en->GetCurrent(&part);
    if(FAILED(hr)) return hr;
    // Existing signature markup is managed separately. Signing it would
    // make a new signature dependent on (or invalidate) earlier signatures.
    if(signature_metadata_part(part.Get())) continue;
    ComPtr<IOpcPartUri> uri;
    hr=part->GetName(&uri);
    if(FAILED(hr)) return hr;
    ComPtr<IOpcSignaturePartReference> sref;
    hr=partRefs->Create(uri.Get(),nullptr,OPC_CANONICALIZATION_NONE,&sref);
    if(FAILED(hr)) return hr;
    ++signedParts;
    ComPtr<IOpcRelationshipSet> rels;
    hr=part->GetRelationshipSet(&rels);
    if(FAILED(hr)) return hr;
    hr=append_relationships(relRefs.Get(),uri.Get(),rels.Get());
    if(FAILED(hr)) return hr;
  }
  if(FAILED(hr)) return hr;
  if(signedParts==0) return E_INVALIDARG;
  ComPtr<IOpcRelationshipSet> rootRelations;
  hr=package->GetRelationshipSet(&rootRelations);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcUri> root;
  hr=factory->CreatePackageRootUri(&root);
  if(FAILED(hr)) return hr;
  return append_relationships(relRefs.Get(),root.Get(),rootRelations.Get());
}
// Require every non-signature OPC part to have an authenticated part reference.
// Windows IOpcDigitalSignatureManager::Validate verifies only the referenced
// bytes and otherwise accepts an appended unsigned part. This additional check
// deliberately fails closed rather than upgrading a partial signature to PASS.
bool signature_metadata_part(IOpcPart* part) {
  LPWSTR ct=nullptr;
  if(FAILED(part->GetContentType(&ct))) return false;
  std::wstring typ=ct?ct:L"";
  CoTaskMemFree(ct);
  ComPtr<IOpcPartUri> uri;
  if(FAILED(part->GetName(&uri))) return false;
  BSTR name=nullptr;
  if(FAILED(uri->GetPath(&name))) return false;
  std::wstring path=name?name:L"";
  SysFreeString(name);
  // Both Office signature containers and WindowsBase PackageDigitalSignatureManager
  // use these designated metadata paths. Other unsigned parts are never exempt.
  if(path.rfind(L"/_xmlsignatures/",0)!=0 &&
     path.rfind(L"/package/services/digital-signature/",0)!=0) return false;
  return typ==L"application/vnd.openxmlformats-package.digital-signature-origin"
      || typ==L"application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml"
      || typ==L"application/vnd.openxmlformats-package.digital-signature-certificate";
}
HRESULT signature_metadata_allowlist(IOpcDigitalSignatureManager* manager,
                                    std::vector<ComPtr<IOpcPartUri>>& allowed) {
  ComPtr<IOpcPartUri> origin;
  HRESULT hr=manager->GetSignatureOriginPartName(&origin);
  if(FAILED(hr)) return hr;
  if(origin) allowed.push_back(origin);
  ComPtr<IOpcDigitalSignatureEnumerator> signatures;
  hr=manager->GetSignatureEnumerator(&signatures);
  if(FAILED(hr)) return hr;
  BOOL more=FALSE;
  while(SUCCEEDED(hr=signatures->MoveNext(&more))&&more) {
    ComPtr<IOpcDigitalSignature> entry;
    hr=signatures->GetCurrent(&entry);
    if(FAILED(hr)) return hr;
    ComPtr<IOpcPartUri> part;
    hr=entry->GetSignaturePartName(&part);
    if(FAILED(hr)) return hr;
    allowed.push_back(part);
    if(allowed.size()>200000) return E_INVALIDARG;
  }
  return hr;
}
HRESULT verify_part_coverage(IOpcPackage* package, IOpcDigitalSignatureManager* manager,
                             IOpcDigitalSignature* signature) {
  std::vector<ComPtr<IOpcPartUri>> approvedMetadata;
  HRESULT metaHr=signature_metadata_allowlist(manager,approvedMetadata);
  if(FAILED(metaHr)) return metaHr;

  ComPtr<IOpcSignaturePartReferenceEnumerator> refs;
  HRESULT hr=signature->GetSignaturePartReferenceEnumerator(&refs);
  if(FAILED(hr)) return hr;
  std::vector<ComPtr<IOpcPartUri>> signedUris;
  BOOL more=FALSE;
  while(SUCCEEDED(hr=refs->MoveNext(&more)) && more) {
    ComPtr<IOpcSignaturePartReference> reference;
    hr=refs->GetCurrent(&reference);
    if(FAILED(hr)) return hr;
    ComPtr<IOpcPartUri> name;
    hr=reference->GetPartName(&name);
    if(FAILED(hr)) return hr;
    signedUris.push_back(name);
    if(signedUris.size()>200000) return E_INVALIDARG;
  }
  if(FAILED(hr)||signedUris.empty()) return FAILED(hr)?hr:TRUST_E_BAD_DIGEST;
  ComPtr<IOpcPartSet> set;
  hr=package->GetPartSet(&set);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcPartEnumerator> parts;
  hr=set->GetEnumerator(&parts);
  if(FAILED(hr)) return hr;
  int checked=0;
  while(SUCCEEDED(hr=parts->MoveNext(&more)) && more) {
    ComPtr<IOpcPart> part;
    hr=parts->GetCurrent(&part);
    if(FAILED(hr)) return hr;
    if(signature_metadata_part(part.Get())) {
      // Even a signature MIME type under a conventional signature folder is
      // not enough: its precise URI must be an actual OPC origin/signature
      // part reported by the verified signature manager.
      ComPtr<IOpcPartUri> metadataUri;
      hr=part->GetName(&metadataUri);
      if(FAILED(hr)) return hr;
      bool isLinked=false;
      for(const auto& trustedPart:approvedMetadata) {
        INT32 comparison=1;
        hr=metadataUri->ComparePartUri(trustedPart.Get(),&comparison);
        if(FAILED(hr)) return hr;
        if(comparison==0) {isLinked=true;break;}
      }
      if(!isLinked) return TRUST_E_BAD_DIGEST;
      continue;
    }
    ComPtr<IOpcPartUri> partUri;
    hr=part->GetName(&partUri);
    if(FAILED(hr)) return hr;
    bool authenticated=false;
    for(const auto& known:signedUris) {
      INT32 eq=1;
      hr=partUri->ComparePartUri(known.Get(),&eq);
      if(FAILED(hr)) return hr;
      if(eq==0) {authenticated=true;break;}
    }
    if(!authenticated) return TRUST_E_BAD_DIGEST;
    if(++checked>200000) return E_INVALIDARG;
  }
  return FAILED(hr)?hr:S_OK;
}
HRESULT crypto_verify(IOpcPackage* package, IOpcDigitalSignatureManager* manager, int* checked) {
  ComPtr<IOpcDigitalSignatureEnumerator> en;
  HRESULT hr=manager->GetSignatureEnumerator(&en);
  if(FAILED(hr)) return hr;
  BOOL more=FALSE;
  int valid=0;
  while(SUCCEEDED(hr=en->MoveNext(&more)) && more) {
    ComPtr<IOpcDigitalSignature> sig;
    hr=en->GetCurrent(&sig);
    if(FAILED(hr)) return hr;
    ComPtr<IOpcCertificateEnumerator> certs;
    hr=sig->GetCertificateEnumerator(&certs);
    if(FAILED(hr)) return hr;
    BOOL hasCert=FALSE;
    bool thisSigValid=false;
    while(SUCCEEDED(hr=certs->MoveNext(&hasCert)) && hasCert) {
      const CERT_CONTEXT* certificate=nullptr;
      hr=certs->GetCurrent(&certificate);
      if(FAILED(hr)) return hr;
      OPC_SIGNATURE_VALIDATION_RESULT verdict=OPC_SIGNATURE_INVALID;
      HRESULT check=manager->Validate(sig.Get(),certificate,&verdict);
      if(SUCCEEDED(check)&&verdict==OPC_SIGNATURE_VALID) thisSigValid=true;
    }
    if(FAILED(hr)) return hr;
    if(!thisSigValid) return TRUST_E_BAD_DIGEST;
    hr=verify_part_coverage(package,manager,sig.Get());
    if(FAILED(hr)) return hr;
    ++valid;
  }
  if(FAILED(hr)) return hr;
  if(valid<=0) return TRUST_E_NOSIGNATURE;
  *checked=valid;
  return S_OK;
}
} // namespace

extern "C" __declspec(dllexport) long __cdecl craft_opc_verify_inprocess(
    const wchar_t* source,int* signaturesChecked) noexcept {
  if(!absolute_path(source)||!signaturesChecked) return E_INVALIDARG;
  *signaturesChecked=0;
  ComInit com;
  if(FAILED(com.hr)&&com.hr!=RPC_E_CHANGED_MODE) return com.hr;
  ComPtr<IOpcFactory> factory;
  HRESULT hr=make_factory(factory);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcPackage> pkg;
  hr=load_package(factory.Get(),source,pkg);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcDigitalSignatureManager> mgr;
  hr=factory->CreateDigitalSignatureManager(pkg.Get(),&mgr);
  if(FAILED(hr)) return hr;
  return crypto_verify(pkg.Get(),mgr.Get(),signaturesChecked);
}


constexpr char kCraftFinalPolicy[] =
    "<craft:signaturePolicy xmlns:craft=\"urn:craft:signing:policy:v1\" allowAdditionalSignatures=\"false\"/>";
// Application-specific rule. Signed and enforced by Craft, not a global
// restriction on other software able to rewrite an OPC signature container.
HRESULT check_and_add_craft_final_policy(
    IOpcFactory* factory, IOpcPackage* package, bool finalSigning, bool alreadySigned) {
  ComPtr<IOpcPartSet> parts;
  HRESULT hr=package->GetPartSet(&parts);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcPartUri> uri;
  hr=factory->CreatePartUri(L"/craft/signature-policy.xml",&uri);
  if(FAILED(hr)) return hr;
  BOOL exists=FALSE;
  hr=parts->PartExists(uri.Get(),&exists);
  if(FAILED(hr)) return hr;
  // An existing policy is immutable even when malformed or unsigned.
  // Never append a signature to a Craft-locked package.
  if(exists) return HRESULT_FROM_WIN32(ERROR_ACCESS_DENIED);
  if(!finalSigning) return S_OK;
  if(alreadySigned) return HRESULT_FROM_WIN32(ERROR_INVALID_STATE);
  ComPtr<IOpcPart> part;
  hr=parts->CreatePart(uri.Get(),L"application/vnd.craft.signing-policy+xml",
                       OPC_COMPRESSION_NORMAL,&part);
  if(FAILED(hr)) return hr;
  ComPtr<IStream> stream;
  hr=part->GetContentStream(&stream);
  if(FAILED(hr)) return hr;
  ULONG written=0;
  constexpr ULONG len=static_cast<ULONG>(sizeof(kCraftFinalPolicy)-1);
  hr=stream->Write(kCraftFinalPolicy,len,&written);
  if(FAILED(hr)) return hr;
  if(written!=len) return STG_E_WRITEFAULT;
  return S_OK;
}

extern "C" __declspec(dllexport) long __cdecl craft_opc_sign_with_policy_inprocess(
    const wchar_t* source,const wchar_t* destination,const wchar_t* sha1Thumbprint, int denyAdditional) noexcept {
  if(!absolute_path(source)||!absolute_path(destination)||wcscmp(source,destination)==0)
    return E_INVALIDARG;
  craft_trace_stage("begin");
  BYTE digest[20]={};
  if(!parse_sha1(sha1Thumbprint,digest)) return E_INVALIDARG;
  if(GetFileAttributesW(destination)!=INVALID_FILE_ATTRIBUTES) return HRESULT_FROM_WIN32(ERROR_FILE_EXISTS);
  ComInit com;
  if(FAILED(com.hr)&&com.hr!=RPC_E_CHANGED_MODE) return com.hr;
  craft_trace_stage("com_ready",com.hr);
  ComPtr<IOpcFactory> factory;
  HRESULT hr=make_factory(factory);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcPackage> pkg;
  hr=load_package(factory.Get(),source,pkg);
  craft_trace_stage("package_loaded",hr);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcDigitalSignatureManager> manager;
  hr=factory->CreateDigitalSignatureManager(pkg.Get(),&manager);
  if(FAILED(hr)) return hr;
  ComPtr<IOpcDigitalSignatureEnumerator> existing;
  hr=manager->GetSignatureEnumerator(&existing);
  if(FAILED(hr)) return hr;
  BOOL already=FALSE;
  hr=existing->MoveNext(&already);
  if(FAILED(hr)) return hr;
  // Office-compatible OPC origin: use the conventional OOXML part name
  // BEFORE any signature is generated. Never rewrite signed packages:
  // moving a signature after signing would invalidate cryptographic coverage.
  constexpr wchar_t kOfficeSignatureOrigin[] = L"/_xmlsignatures/origin.sigs";
  ComPtr<IOpcPartUri> officeOrigin;
  hr=factory->CreatePartUri(kOfficeSignatureOrigin,&officeOrigin);
  if(FAILED(hr)) return hr;
  int priorCount=0;
  if(already) {
    // Existing signatures must already be valid before another is appended.
    hr=crypto_verify(pkg.Get(),manager.Get(),&priorCount);
    if(FAILED(hr)) return hr;
    if(priorCount<=0 || priorCount>=32) return E_INVALIDARG;
    // The origin name of an already-signed package is immutable. To avoid
    // silently appending to a generic OPC-only signed package, fail closed.
    ComPtr<IOpcPartUri> existingOrigin;
    hr=manager->GetSignatureOriginPartName(&existingOrigin);
    if(FAILED(hr)||!existingOrigin) return FAILED(hr)?hr:E_INVALIDARG;
    BSTR existingPath=nullptr;
    hr=existingOrigin->GetPath(&existingPath);
    if(FAILED(hr)) return hr;
    const bool compatible=existingPath &&
      _wcsicmp(existingPath,kOfficeSignatureOrigin)==0;
    if(existingPath) SysFreeString(existingPath);
    if(!compatible) return HRESULT_FROM_WIN32(ERROR_NOT_SUPPORTED);
    craft_trace_stage("office_origin_existing_validated");
  } else {
    hr=manager->SetSignatureOriginPartName(officeOrigin.Get());
    craft_trace_stage("office_origin_assigned",hr);
    if(FAILED(hr)) return hr;
  }
  // Refuse Craft policy-locked packages in both signing modes.
  // For a new final signature, insert the signed policy before collecting
  // package part references, so this new part is cryptographically covered.
  hr=check_and_add_craft_final_policy(factory.Get(),pkg.Get(),denyAdditional!=0,already!=FALSE);
  craft_trace_stage("final_policy_prepared",hr);
  if(FAILED(hr)) return hr;
  HCERTSTORE certStore=CertOpenStore(CERT_STORE_PROV_SYSTEM_W,0,0,CERT_SYSTEM_STORE_CURRENT_USER,L"MY");
  if(!certStore) return HRESULT_FROM_WIN32(GetLastError());
  CRYPT_HASH_BLOB thumbprint={20,digest};
  PCCERT_CONTEXT cert=CertFindCertificateInStore(certStore,X509_ASN_ENCODING|PKCS_7_ASN_ENCODING,
    0,CERT_FIND_SHA1_HASH,&thumbprint,nullptr);
  if(!cert) { CertCloseStore(certStore,0); return HRESULT_FROM_WIN32(ERROR_NOT_FOUND); }
  craft_trace_stage("certificate_found");
  ComPtr<IOpcSigningOptions> options;
  hr=manager->CreateSigningOptions(&options);
  // Embed the X.509 certificate inside the authenticated signature part.
  // This avoids an additional unsigned certificate part while preserving
  // package-wide part coverage checks for all document content.
  if(SUCCEEDED(hr)) hr=options->SetCertificateEmbeddingOption(OPC_CERTIFICATE_IN_SIGNATURE_PART);
  // SignatureProperty.Target must resolve to the actual root ds:Signature Id.
  if(SUCCEEDED(hr)) hr=options->SetSignatureId(L"idPackageSignature");
  if(SUCCEEDED(hr)) hr=options->SetDefaultDigestMethod(L"http://www.w3.org/2001/04/xmlenc#sha256");
  if(SUCCEEDED(hr)) hr=options->SetSignatureMethod(L"http://www.w3.org/2001/04/xmldsig-more#rsa-sha256");
  // Set Office-standard per-document signature-part path, before signing.
  wchar_t signaturePartPath[72]={};
  if(SUCCEEDED(hr) && swprintf_s(signaturePartPath,L"/_xmlsignatures/sig%d.xml",priorCount+1)<0)
    hr=E_INVALIDARG;
  ComPtr<IOpcPartUri> signaturePart;
  if(SUCCEEDED(hr)) hr=factory->CreatePartUri(signaturePartPath,&signaturePart);
  if(SUCCEEDED(hr)) hr=options->SetSignaturePartName(signaturePart.Get());
  if(SUCCEEDED(hr)) hr=add_office_signature_object(options.Get());
  craft_trace_stage("office_custom_signature_object",hr);
  if(SUCCEEDED(hr)) hr=sign_parts(factory.Get(),pkg.Get(),options.Get());
  craft_trace_stage("references_prepared",hr);
  // Do not call CryptAcquireCertificatePrivateKey before OPC Sign.
  // B-Trust middleware has blocked there with no PIN window on an initial
  // and on a subsequent signature. IOpcDigitalSignatureManager::Sign is
  // responsible for the sole provider interaction and propagates its HRESULT.
  craft_trace_stage("provider_preacquire_disabled");
  if(SUCCEEDED(hr)) {
    ComPtr<IOpcDigitalSignature> signature;
    // User's Windows crypto provider may show its normal PIN prompt here.
    craft_trace_stage("provider_sign_enter");
    hr=manager->Sign(cert,options.Get(),&signature);
    craft_trace_stage("provider_sign_return",hr);
  }
  CertFreeCertificateContext(cert);
  CertCloseStore(certStore,0);
  if(FAILED(hr)) return hr;
  craft_trace_stage("provider_signature_done");
  GUID guid;
  hr=CoCreateGuid(&guid);
  if(FAILED(hr)) return hr;
  wchar_t uuid[40]={};
  StringFromGUID2(guid,uuid,40);
  std::wstring temp=std::wstring(destination)+L".tmp"+uuid;
  ComPtr<IStream> out;
  hr=factory->CreateStreamOnFile(temp.c_str(),OPC_STREAM_IO_WRITE,nullptr,FILE_ATTRIBUTE_NORMAL,&out);
  // OPC_WRITE_DEFAULT serializes even small documents as Zip64 (minimum reader version 4.5).
  // Prefer Zip32 for Office interoperability, failing closed when Zip32 limits are exceeded.
  if(SUCCEEDED(hr)) hr=factory->WritePackageToStream(pkg.Get(),OPC_WRITE_FORCE_ZIP32,out.Get());
  craft_trace_stage("package_serialized",hr);
  out.Reset();
  if(SUCCEEDED(hr)) {
    int valid=0;
    // The result is reopened independently, not only verified in memory.
    craft_trace_stage("verification_enter");
    long check=craft_opc_verify_inprocess(temp.c_str(),&valid);
    craft_trace_stage("verification_return",check);
    if(FAILED(check)||valid!=priorCount+1) {
      hr=FAILED(check)?check:TRUST_E_BAD_DIGEST;
    }
  }
  if(SUCCEEDED(hr)&&!MoveFileExW(temp.c_str(),destination,MOVEFILE_WRITE_THROUGH))
    hr=HRESULT_FROM_WIN32(GetLastError());
  if(FAILED(hr)) DeleteFileW(temp.c_str());

  craft_trace_stage("finished",hr);
  return hr;
}
extern "C" __declspec(dllexport) long __cdecl craft_opc_sign_inprocess(
    const wchar_t* source,const wchar_t* destination,const wchar_t* sha1Thumbprint) noexcept {
  return craft_opc_sign_with_policy_inprocess(source,destination,sha1Thumbprint,0);
}

extern "C" int craft_windows_cert_inventory(char* output, int capacity) {
  if (!output || capacity < 2) return -1;
  HCERTSTORE store=CertOpenSystemStoreW(0,L"MY");
  if (!store) return -2;
  std::string data="[";
  PCCERT_CONTEXT cert=nullptr;
  bool first=true;
  while ((cert=CertEnumCertificatesInStore(store,cert))!=nullptr) {
    BYTE hash[20]={}; DWORD size=sizeof(hash);
    if (!CertGetCertificateContextProperty(cert,CERT_SHA1_HASH_PROP_ID,hash,&size) || size!=20) continue;
    wchar_t subject[512]={}, issuer[512]={};
    CertGetNameStringW(cert,CERT_NAME_SIMPLE_DISPLAY_TYPE,0,nullptr,subject,512);
    CertGetNameStringW(cert,CERT_NAME_SIMPLE_DISPLAY_TYPE,CERT_NAME_ISSUER_FLAG,nullptr,issuer,512);
    auto escape=[](const wchar_t* wide) {
      int len=WideCharToMultiByte(CP_UTF8,0,wide,-1,nullptr,0,nullptr,nullptr);
      if(len<=0) return std::string();
      std::string utf8(size_t(len),'\0');
      WideCharToMultiByte(CP_UTF8,0,wide,-1,utf8.data(),len,nullptr,nullptr);
      utf8.pop_back();
      std::string escaped;
      for(unsigned char ch:utf8) {
        if(ch=='"' || ch=='\\') { escaped+='\\'; escaped+=char(ch); }
        else if(ch<32) { char buf[8]; sprintf_s(buf,"\\u%04X",unsigned(ch)); escaped+=buf; }
        else escaped+=char(ch);
      }
      return escaped;
    };
    char thumb[41]={};
    for(int i=0;i<20;i++) sprintf_s(thumb+2*i,41-2*i,"%02X",unsigned(hash[i]));
    DWORD keysize=0;
    bool haskey=CertGetCertificateContextProperty(cert,CERT_KEY_PROV_INFO_PROP_ID,nullptr,&keysize)!=FALSE;
    SYSTEMTIME now={}; GetSystemTime(&now);
    FILETIME ft={}; SystemTimeToFileTime(&now,&ft);
    bool valid=CertVerifyTimeValidity(&ft,cert->pCertInfo)==0;
    SYSTEMTIME until={}; FileTimeToSystemTime(&cert->pCertInfo->NotAfter,&until);
    char date[32]; sprintf_s(date,"%04u-%02u-%02u",until.wYear,until.wMonth,until.wDay);
    if(!first) data+=",";
    first=false;
    data+="{\"thumbprint\":\""+std::string(thumb)+"\",\"subject\":\""+escape(subject)+"\",\"issuer\":\""+escape(issuer)+"\",\"expires\":\""+std::string(date)+"\",\"has_private_key\":"+(haskey?"true":"false")+",\"valid_now\":"+(valid?"true":"false")+"}";
  }
  CertCloseStore(store,0);
  data+="]";
  if(data.size()+1>size_t(capacity)) return -3;
  memcpy(output,data.c_str(),data.size()+1);
  return int(data.size());
}
