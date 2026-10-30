//! Rebuild the Office [Content_Types].xml directory with a conventional XML
//! default while preserving every part's effective MIME (OPC signature input).
//! Caller must independently validate the signed package after normalization.
#![cfg(windows)]
use std::{collections::{BTreeMap, HashMap, HashSet},fs::{File,OpenOptions},io::{Read,Write},path::Path};
use zip::{ZipArchive,ZipWriter,CompressionMethod,write::SimpleFileOptions};

const NS: &str="http://schemas.openxmlformats.org/package/2006/content-types";
fn attr(s:&str)->String {
    s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;")
     .replace('"',"&quot;").replace('\'',"&apos;")
}
fn normalized_mime_table(original:&str, names:&[String])->Result<Vec<u8>,String>{
    let xml=roxmltree::Document::parse(original).map_err(|e|e.to_string())?;
    let root=xml.root_element();
    if root.tag_name().name()!="Types" || root.tag_name().namespace()!=Some(NS){
        return Err("Unexpected OPC MIME XML namespace".into());
    }
    let mut defaults=HashMap::new();
    let mut overrides=HashMap::new();
    for child in root.children().filter(|c|c.is_element()){
        let mime=child.attribute("ContentType").ok_or("ContentType missing")?;
        match child.tag_name().name(){
            "Default"=>{
                let ext=child.attribute("Extension").ok_or("Extension missing")?;
                if defaults.insert(ext.to_string(),mime.to_string()).is_some(){
                    return Err("Duplicate MIME default".into());
                }
            }
            "Override"=>{
                let name=child.attribute("PartName").ok_or("PartName missing")?;
                if overrides.insert(name.to_string(),mime.to_string()).is_some(){
                    return Err("Duplicate MIME override".into());
                }
            }
            _=>return Err("Unsupported OPC MIME child".into())
        }
    }
    let mut resolved=BTreeMap::new();
    for name in names.iter().filter(|p|p.as_str()!="[Content_Types].xml" && !p.ends_with('/')){
        let ext=name.rsplit_once('.').map(|(_,e)|e).unwrap_or("");
        let mime=overrides.get(&format!("/{name}")).or_else(||defaults.get(ext))
            .ok_or_else(||format!("Missing MIME for {name}"))?;
        resolved.insert(name.clone(),mime.clone());
    }
    defaults.insert("xml".into(),"application/xml".into());
    let default:BTreeMap<_,_>=defaults.into_iter().collect();
    let mut out=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"{NS}\">");
    for (ext,mime) in &default{
        out.push_str(&format!("<Default Extension=\"{}\" ContentType=\"{}\"/>",attr(ext),attr(mime)));
    }
    for (name,mime) in resolved {
        let ext=name.rsplit_once('.').map(|(_,e)|e).unwrap_or("");
        if default.get(ext)!=Some(&mime){
            out.push_str(&format!("<Override PartName=\"/{}\" ContentType=\"{}\"/>",
                attr(&name),attr(&mime)));
        }
    }
    out.push_str("</Types>");
    Ok(out.into_bytes())
}
pub(crate) fn normalize_office_content_types(source:&Path,dest:&Path)->Result<(),String>{
    if source==dest || dest.exists(){return Err("Office normalization needs a new output".into());}
    let mut archive=ZipArchive::new(File::open(source).map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())?;
    if archive.len()>65535 || archive.is_empty(){return Err("ZIP32 entry limit".into());}
    let mut names=Vec::new();
    let mut seen=HashSet::new();
    let mut total=0u64;
    for i in 0..archive.len(){
        let p=archive.by_index(i).map_err(|e|e.to_string())?;
        let name=p.name().to_owned();
        if name.is_empty() || name.starts_with('/') || name.contains('\\') ||
           name.split('/').any(|x|x==".." || x==".") || !seen.insert(name.clone()){
            return Err("Unsafe OPC part name".into());
        }
        if !matches!(p.compression(),CompressionMethod::Stored|CompressionMethod::Deflated){
            return Err("Unsupported ZIP compression".into());
        }
        total=total.checked_add(p.size()).ok_or("ZIP size overflow")?;
        if total>1024*1024*1024 {return Err("ZIP inflated size above 1GiB".into());}
        names.push(name);
    }
    let mut ct=String::new();
    archive.by_name("[Content_Types].xml").map_err(|e|e.to_string())?
        .read_to_string(&mut ct).map_err(|e|e.to_string())?;
    let normalized=normalized_mime_table(&ct,&names)?;
    let file=OpenOptions::new().write(true).create_new(true).open(dest)
        .map_err(|e|e.to_string())?;
    let mut out=ZipWriter::new(file);
    for index in 0..archive.len(){
        let mut entry=archive.by_index(index).map_err(|e|e.to_string())?;
        let opts=SimpleFileOptions::default().compression_method(entry.compression()).large_file(false);
        if entry.is_dir(){
            out.add_directory(entry.name(),opts).map_err(|e|e.to_string())?;
        }else{
            let name=entry.name().to_owned();
            out.start_file(&name,opts).map_err(|e|e.to_string())?;
            if name=="[Content_Types].xml"{out.write_all(&normalized).map_err(|e|e.to_string())?;}
            else{std::io::copy(&mut entry,&mut out).map_err(|e|e.to_string())?;}
        }
    }
    out.finish().map_err(|e|e.to_string())?.sync_all().map_err(|e|e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn signature_mime_is_override_and_doc_parts_preserve_their_mime(){
        let xml=r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml"/><Default Extension="sigs" ContentType="application/vnd.openxmlformats-package.digital-signature-origin"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>"#;
        let names=vec!["_xmlsignatures/origin.sigs".into(),"_xmlsignatures/sig1.xml".into(),"xl/workbook.xml".into()];
        let s=String::from_utf8(normalized_mime_table(xml,&names).unwrap()).unwrap();
        assert!(s.contains("Extension=\"xml\" ContentType=\"application/xml\""));
        assert!(s.contains("PartName=\"/_xmlsignatures/sig1.xml\" ContentType=\"application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml\""));
        assert!(s.contains("PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\""));
        assert!(s.contains("Extension=\"sigs\" ContentType=\"application/vnd.openxmlformats-package.digital-signature-origin\""));
    }
}
