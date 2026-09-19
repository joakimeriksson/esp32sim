//! Host input boundaries must behave identically in native tests and the wasm32 build.
use esp32sim_wasm::*;

#[test]
fn memory_sizes_are_rejected_before_allocating_or_truncating() {
    for size in [33, 2048, 4096, u32::MAX] {
        // SAFETY: The name is readable and rejected construction retains no allocation.
        assert!(unsafe { esp32sim_new(b"none".as_ptr(), 4, size, 0) }.is_null());
        assert!(unsafe { esp32sim_new(b"none".as_ptr(), 4, 1, size) }.is_null());
    }
    let net = esp32sim_net_new(0.0);
    let mac = [2, 0, 0, 0, 0, 1];
    // SAFETY: This test uniquely owns the network and provides valid input buffers.
    unsafe {
        for size in [33, 4096, u32::MAX] {
            assert_eq!(esp32sim_net_add(net, mac.as_ptr(), size, 0.0, 0.0, 0.0, b"none".as_ptr(), 4), u32::MAX);
        }
        // Failed additions did not leave partially initialized nodes behind.
        assert_eq!(esp32sim_net_add(net, mac.as_ptr(), 1, 0.0, 0.0, 0.0, b"none".as_ptr(), 4), 0);
        esp32sim_net_delete(net);
    }
}

#[test]
fn single_and_network_loads_accept_the_same_input_kinds() {
    // SAFETY: Buffers stay readable and handles are uniquely owned throughout the test.
    unsafe {
        let single = esp32sim_new(b"esp32c6".as_ptr(), 7, 1, 0);
        assert!(!single.is_null());
        let net = esp32sim_net_new(0.0);
        let mac = [2, 0, 0, 0, 0, 1];
        let node = esp32sim_net_add(net, mac.as_ptr(), 1, 0.0, 0.0, 0.0, b"none".as_ptr(), 4);
        let cases: &[(u32, &[u8], u32)] = &[
            (6, b"0.1 gpio 2 1\n", 0),
            (6, &[0xff], 1),
            (7, b"P6\n1 1\n255\n\xff\x00\x00", 0),
            (7, b"bad picture", 1),
            (8, b"", 1),
        ];
        for &(kind, data, expected) in cases {
            assert_eq!(esp32sim_load(single, kind, data.as_ptr(), data.len()), expected);
            assert_eq!(esp32sim_net_load(net, node, kind, data.as_ptr(), data.len()), expected);
        }
        for name in ["0x40370000", "40370000", "missing_symbol"] {
            let expected = u32::from(name == "missing_symbol");
            assert_eq!(esp32sim_stub(single, name.as_ptr(), name.len(), 0), expected);
            assert_eq!(esp32sim_net_stub(net, node, name.as_ptr(), name.len(), 0), expected);
        }
        esp32sim_delete(single);
        esp32sim_net_delete(net);
    }
}

#[test]
fn stub_symbols_take_precedence_over_hexadecimal_spelling() {
    let mut machine = esp32c6::machine([0; 6], 1 << 20);
    machine.symbols.insert(0x1234, "deadbeef".into());
    machine.symbols.insert(0x5678, "0xcafe".into());
    assert_eq!(machine.resolve_stub("deadbeef"), Some(0x1234));
    assert_eq!(machine.resolve_stub("0xcafe"), Some(0x5678));
    assert_eq!(machine.resolve_stub("cafe"), Some(0xcafe));
    assert_eq!(machine.resolve_stub("0x0xcafe"), None);
}

#[test]
fn board_aliases_route_to_the_canonical_chip() {
    for (board, hz) in [("bare", 240e6), ("c3", 160e6), ("c6", 160e6), ("lcd147", 160e6), ("c6-lcd147", 160e6)] {
        // SAFETY: The name is readable and the handle is uniquely owned until deletion.
        unsafe {
            let e = esp32sim_new(board.as_ptr(), board.len(), 1, 0);
            assert!(!e.is_null(), "{board}");
            assert_eq!(esp32sim_cpu_hz(e), hz, "{board}");
            assert_eq!(esp32sim_set_spi2_timing(e, 1), u32::from(hz != 240e6));
            esp32sim_delete(e);
        }
    }
}
