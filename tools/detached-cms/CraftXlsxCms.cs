// Craft GridCraft detached CMS/PKCS#7 for unchanged XLSX bytes.
// Signature is external (.p7s), NOT an Office XML-DSig. Trust and QES require separate checks.
using System;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Security.Cryptography.Pkcs;
using System.Security.Cryptography.X509Certificates;
public static class CraftXlsxCms {
    const long MaxBytes=128L*1024*1024, MaxSignature=2L*1024*1024;
    static string Hash(byte[] data) {
        using(var sha=SHA256.Create()) return BitConverter.ToString(sha.ComputeHash(data)).Replace("-","");
    }
    static byte[] Load(string path,long maximum) {
        var fi=new FileInfo(path);
        if(!fi.Exists || fi.Length==0 || fi.Length>maximum) throw new IOException("Missing/empty/oversized input");
        return File.ReadAllBytes(path);
    }
    static X509Certificate2 Cert(string thumb) {
        using(var store=new X509Store(StoreName.My,StoreLocation.CurrentUser)){
            store.Open(OpenFlags.ReadOnly);
            var normalized=new string(thumb.Where(character=>Uri.IsHexDigit(character)).ToArray());
            var matches=store.Certificates.Find(X509FindType.FindByThumbprint,normalized,false);
            if(matches.Count!=1) throw new InvalidOperationException("Certificate must uniquely exist in CurrentUser/My");
            var c=matches[0];
            if(!c.HasPrivateKey || DateTime.Now<c.NotBefore || DateTime.Now>c.NotAfter)
                throw new InvalidOperationException("Certificate not eligible for signing");
            foreach(X509Extension e in c.Extensions){
                var usage=e as X509KeyUsageExtension;
                if(usage!=null && (usage.KeyUsages & X509KeyUsageFlags.DigitalSignature)==0)
                    throw new InvalidOperationException("Digital signature key usage not granted");
            }
            return c;
        }
    }
    static SignedCms VerifyDetached(byte[] content,byte[] signature) {
        var cms=new SignedCms(new ContentInfo(content),true);
        cms.Decode(signature);
        if(!cms.Detached || cms.SignerInfos.Count!=1)
            throw new InvalidOperationException("Expected one detached signer");
        cms.CheckSignature(true); // signature math only, not PKI trust or eIDAS QES
        return cms;
    }
    static void Sign(string xlsx,string signaturePath,string thumb) {
        if(!Path.GetExtension(xlsx).Equals(".xlsx",StringComparison.OrdinalIgnoreCase)
            || !Path.GetExtension(signaturePath).Equals(".p7s",StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Expected .xlsx input and .p7s output");
        if(File.Exists(signaturePath)) throw new IOException("Refusing existing signature file");
        var data=Load(xlsx,MaxBytes);
        var digest=Hash(data);
        var signer=new CmsSigner(SubjectIdentifierType.IssuerAndSerialNumber,Cert(thumb));
        signer.DigestAlgorithm=new Oid("2.16.840.1.101.3.4.2.1");
        signer.IncludeOption=X509IncludeOption.EndCertOnly;
        var cms=new SignedCms(new ContentInfo(data),true);
        cms.ComputeSignature(signer,false); // Windows certificate provider controls PIN prompt
        var signature=cms.Encode();
        VerifyDetached(data,signature);
        if(Hash(Load(xlsx,MaxBytes))!=digest) throw new IOException("XLSX changed during signing");
        bool published=false;
        try {
            using(var file=new FileStream(signaturePath,FileMode.CreateNew,FileAccess.Write,FileShare.None)){
                published=true;file.Write(signature,0,signature.Length);file.Flush(true);
            }
            VerifyDetached(Load(xlsx,MaxBytes),Load(signaturePath,MaxSignature));
            Console.WriteLine("CMS_DETACHED_SIGN=PASS; original_sha256="+digest+"; trust=NOT_EVALUATED; QES=NOT_EVALUATED");
        } catch {if(published)try{File.Delete(signaturePath);}catch{};throw;}
    }
    static void Verify(string xlsx,string signaturePath){
        var data=Load(xlsx,MaxBytes);
        var cms=VerifyDetached(data,Load(signaturePath,MaxSignature));
        var c=cms.SignerInfos[0].Certificate;
        Console.WriteLine("CMS_DETACHED_VERIFY=PASS; sha256="+Hash(data)
          +"; signer="+(c==null?"not_available":c.Subject)+"; trust=NOT_EVALUATED; QES=NOT_EVALUATED");
    }
    public static int Main(string[] args) {
        try {
            if(args.Length==4 && args[0]=="sign"){Sign(args[1],args[2],args[3]);return 0;}
            if(args.Length==3 && args[0]=="verify"){Verify(args[1],args[2]);return 0;}
            Console.Error.WriteLine("Usage: craft-xlsx-cms.exe sign file.xlsx new.p7s thumbprint | verify file.xlsx file.p7s");
            return 2;
        } catch(Exception ex) {
            Console.Error.WriteLine("CMS_FAILED: "+ex.GetType().Name+": "+ex.Message);
            return 1;
        }
    }
}
