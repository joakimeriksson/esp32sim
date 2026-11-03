// `--adc` / `--adc-raw` are host options: they parse before firmware, and the observation line
// is printed at exit even when the pad was never sampled.
fn empty_app(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("app.bin");
    let mut image = vec![0u8; 24];
    image[0] = 0xe9;
    image[4..8].copy_from_slice(&0x4037_0000u32.to_le_bytes());
    std::fs::write(&path, image).unwrap();
    path
}

#[test]
fn invalid_adc_options_exit_as_usage_errors() {
    for args in [
        vec!["--adc"],
        vec!["--adc-raw"],
        vec!["--adc", "wrong"],
        vec!["--adc", "-1"],
        vec!["--adc", "256"],
        vec!["--adc-raw", "1"],
        vec!["--adc-raw", "1=4096"],
        vec!["--adc-raw", "x=1"],
        vec!["--adc-raw", "1=-1"],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_esp32sim")).args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(!stderr.contains("panicked"), "{args:?}: {stderr}");
        assert!(stderr.contains(if args.len() == 1 { "usage:" } else { "--adc" }), "{args:?}: {stderr}");
    }
}

#[test]
fn adc_observation_is_printed_for_pads_and_refused_for_the_rest() {
    let dir = std::env::temp_dir().join(format!("esp32sim-adc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let app = empty_app(&dir);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_esp32sim"))
        .args(["--board", "none", "--boot", "app", "--app", app.to_str().unwrap(), "--flash-mb", "1",
            "--max-insns", "1", "--no-dump", "--adc", "1", "--adc", "99", "--adc-raw", "1=1234"]).output().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert!(stderr.contains("[adc] GPIO1: raw 0, generation 0"), "{stderr}");
    assert!(stderr.contains("[adc] GPIO99: no ADC pad"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn adc_raw_rejects_a_pin_that_is_not_an_adc_pad() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_esp32sim"))
        .args(["--board", "none", "--adc-raw", "99=1", "--max-insns", "1"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--adc-raw: GPIO99 is not an ADC pad"), "{stderr}");
}
