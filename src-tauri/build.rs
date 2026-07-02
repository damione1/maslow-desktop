fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri_build::build();

    let out_dir = std::path::PathBuf::from("src/generated");
    std::fs::create_dir_all(&out_dir)?;

    let descriptor_path = out_dir.join("maslow_descriptor.bin");

    // Generated output is gitignored, so a missing file here (fresh clone or
    // manual `rm -rf src/generated`) must force a rerun even though the
    // proto sources themselves haven't changed.
    println!(
        "cargo:rerun-if-changed={}",
        out_dir.join("maslow.v1.rs").display()
    );

    let proto_files = [
        "../proto/maslow/v1/common.proto",
        "../proto/maslow/v1/calibration.proto",
        "../proto/maslow/v1/config.proto",
        "../proto/maslow/v1/files.proto",
        "../proto/maslow/v1/job.proto",
        "../proto/maslow/v1/machine.proto",
    ];
    let proto_includes = ["../proto"];

    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .out_dir(&out_dir)
        .file_descriptor_set_path(&descriptor_path)
        .compile_protos(&proto_files, &proto_includes)?;

    pbjson_build::Builder::new()
        .register_descriptors(&std::fs::read(&descriptor_path)?)?
        .out_dir(&out_dir)
        .build(&[".maslow.v1"])?;

    Ok(())
}
