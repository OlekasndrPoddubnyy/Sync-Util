fn main() {
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent".into()),
    )
    .expect("compilazione dell'interfaccia Slint non riuscita");

    // icona e dettagli dell'eseguibile, visibili in Esplora risorse e nella barra delle applicazioni
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=ui/icons/app.ico");
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("ui/icons/app.ico")
            .set("FileDescription", "Dimensions Sync")
            .set("ProductName", "Dimensions Sync");
        resource.compile().expect("compilazione delle risorse Windows non riuscita");
    }
}
