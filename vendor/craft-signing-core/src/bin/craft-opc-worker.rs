//! Dedicated short-lived Windows OPC worker. Never used for key export or PIN entry.
//! The parent process enforces an absolute timeout and publishes the final file only
//! after the child exits cleanly and the signed staging file verifies successfully.
#[cfg(windows)]
fn main() {
    use std::{env, path::PathBuf};
    let args:Vec<_> = env::args_os().collect();
    if args.len()!=6 || args[1]!="--craft-worker-sign-v1" { std::process::exit(64); }
    let Some(thumb)=args[2].to_str() else {std::process::exit(64)};
    let mode=match args[3].to_str(){Some("allow")=>true,Some("craft-final")=>false,_=>std::process::exit(64)};
    let source=PathBuf::from(&args[4]);let destination=PathBuf::from(&args[5]);
    let code=match craft_signing_core::opc_native::sign_opc_worker_direct(
        &source,&destination,thumb,mode){Ok(())=>0,Err(_)=>2};
    std::process::exit(code);
}
#[cfg(not(windows))]
fn main(){std::process::exit(64)}
