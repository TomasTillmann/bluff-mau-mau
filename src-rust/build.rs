fn main() {
    let output =
        std::process::Command::new(std::env::var_os("RUSTC").expect("Cargo supplies rustc"))
            .arg("--version")
            .output()
            .expect("Read compiler version");
    assert!(output.status.success());
    println!(
        "cargo:rustc-env=RUSTC_VERSION={}",
        String::from_utf8(output.stdout).unwrap().trim()
    );
    println!("cargo:rerun-if-changed=build.rs");
}
