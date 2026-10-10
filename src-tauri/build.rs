fn main() {
    println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    tauri_build::build();
}
