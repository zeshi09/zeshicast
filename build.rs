fn main() {
    if std::env::var_os("CARGO_FEATURE_LAYER_SHELL").is_none() {
        return;
    }

    let output = std::process::Command::new("pkg-config")
        .args(["--libs", "gtk4-layer-shell-0"])
        .output()
        .expect("pkg-config is required to build with the layer-shell feature");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("gtk4-layer-shell-0 not found via pkg-config: {stderr}");
    }

    let flags = String::from_utf8(output.stdout).unwrap();
    for flag in flags.split_whitespace() {
        if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        } else if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={lib}");
        }
    }
}
