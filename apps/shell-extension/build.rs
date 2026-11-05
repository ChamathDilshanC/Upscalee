fn main() {
    // DllGetClassObject / DllCanUnloadNow are looked up by name via GetProcAddress
    // and never imported through a .lib, so the "should be PRIVATE" advice
    // (LNK4104) does not apply.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-cdylib-link-arg=/IGNORE:4104");
    }
}
