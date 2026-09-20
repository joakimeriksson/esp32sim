use super::*;

#[test]
fn function_trace_patterns_share_prefix_and_exact_matching() {
    let mut m = esp32c6::machine([0; 6], 4 << 20);
    m.symbols.extend([(1, "foo".into()), (2, "foobar".into()), (3, "other".into())]);
    assert_eq!(m.trace_fns("foo"), 2);
    assert_eq!(m.fn_probes.len(), 2);
    assert!(m.fn_probes.contains_key(&1));
    assert!(m.fn_probes.contains_key(&2));
    m.fn_probes.clear();
    assert_eq!(m.trace_fns("foo$"), 1);
    assert_eq!(m.fn_probes.keys().copied().collect::<Vec<_>>(), [1]);
    assert_eq!(m.trace_fns("missing$"), 0);
}

#[test]
fn stub_values_are_explicit() {
    for (spec, expected) in [("func", 0), ("func=0", 0), ("func=true", 1), ("func=false", 0), ("func=42", 42), ("func=0x2a", 42)] {
        assert_eq!(stub_spec(spec), Ok(("func", expected)));
    }
    for spec in ["func=typo", "func=", "func=-1", "func=4294967296"] {
        assert!(stub_spec(spec).is_err(), "{spec}");
    }
}

#[test]
fn reset_loop_spends_one_run_budget() {
    let mut m = esp32c6::machine([0; 6], 4 << 20);
    // lui t0,0x600b1; lui t1,0x10000; sw t1,0x38(t0): request CPU reset.
    let program: Vec<u8> = [0x600b12b7u32, 0x10000337, 0x0262ac23].into_iter().flat_map(u32::to_le_bytes).collect();
    m.bus.load_bytes(0x4000_0000, &program).unwrap();
    // A watchdog for the regression itself: the old per-boot budget reaches this cap.
    m.max_cycles = 10_000;
    assert!(matches!(run_with_reboots(&mut m, 10, true), Stop::MaxInsns));
    assert!(m.reboots > 0 && m.reboots < 10, "reboots: {}", m.reboots);
    assert!(m.insns() < 100, "instructions: {}", m.insns());
}

#[test]
fn approximate_options_reject_unsupported_combinations() {
    for chip in ["c3", "c6", "esp32c3", "esp32c6"] {
        for flag in ["--approximate-timing", "--approximate-cache", "--approximate-memory"] {
            let mut args = vec!["esp32sim".into(), flag.into()];
            if flag == "--approximate-memory" { args.push("3".into()); }
            assert!(validate_timing(&parse(&args, chip)).unwrap_err().contains("require --chip s3"));
        }
    }
    let mut o = Opts { chip: "s3".into(), memory_contention: true, ..Default::default() };
    assert!(validate_timing(&o).unwrap_err().contains("requires --approximate-memory"));
    o.approximate_timing = true;
    o.approximate_memory = Some(3);
    assert!(validate_timing(&o).unwrap_err().contains("requires --boot rom"));
    o.boot = Some("rom".into());
    assert!(validate_timing(&o).is_ok());
}

#[test]
fn timing_cycle_values_report_usage_errors() {
    for name in ["--approximate-memory", "ESP32SIM_CACHE_FILL", "ESP32SIM_CACHE_WRITEBACK"] {
        for value in ["", "wrong", "-1", "4294967296"] {
            assert!(timing_cycles(value, name).unwrap_err().starts_with(name));
        }
        assert_eq!(timing_cycles("0", name), Ok(0));
        assert_eq!(timing_cycles("4294967295", name), Ok(u32::MAX));
    }
}
