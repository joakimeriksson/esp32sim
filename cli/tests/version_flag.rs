// A released binary must say which version it is, on stdout, without needing firmware images.
#[test]
fn version_flag_prints_the_crate_version_on_every_binary() {
    let expected = format!("esp32sim {}\n", env!("CARGO_PKG_VERSION"));
    for binary in [env!("CARGO_BIN_EXE_esp32sim"), env!("CARGO_BIN_EXE_esp32sim-c3"), env!("CARGO_BIN_EXE_esp32sim-c6"), env!("CARGO_BIN_EXE_esp32sim-p4")] {
        for flag in ["--version", "-V"] {
            let output = std::process::Command::new(binary).arg(flag).output().unwrap();
            assert_eq!(output.status.code(), Some(0), "{binary} {flag}");
            assert_eq!(String::from_utf8(output.stdout).unwrap(), expected, "{binary} {flag}");
        }
    }
}
