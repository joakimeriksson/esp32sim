//! ROM-free regression tests of the exported C ABI's timing configuration order.
use esp32sim_wasm::*;

struct Emulator(*mut Emu);
impl Emulator {
    fn new(board: &str) -> Self {
        // SAFETY: board is readable for the call; this wrapper owns the returned handle.
        let handle = unsafe { esp32sim_new(board.as_ptr(), board.len(), 4, 2) };
        assert!(!handle.is_null());
        Self(handle)
    }
}
impl Drop for Emulator {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the live handle.
        unsafe { esp32sim_delete(self.0) }
    }
}

#[test]
fn prices_require_approximate_scheduler_not_just_a_cost_model() {
    for cost_model in [false, true] {
        let e = Emulator::new("atech14");
        // SAFETY: all calls exclusively borrow the live handle and retain no pointers.
        unsafe {
            if cost_model { assert_eq!(esp32sim_set_approximate_timing(e.0), 0); }
            assert_eq!(esp32sim_set_approximate_jit_cache(e.0, 96, 40, 0), 1);
            assert_eq!(esp32sim_set_approximate_pie_timing(e.0, 1), 1);
            assert_eq!(esp32sim_set_control_prices(e.0, 1), 1);
            assert_eq!(esp32sim_set_icache_fill(e.0, 32), 1);
            // Turning prices off is harmless even without the approximate scheduler.
            assert_eq!(esp32sim_set_approximate_pie_timing(e.0, 0), 0);
            assert_eq!(esp32sim_set_control_prices(e.0, 0), 0);
            assert_eq!(esp32sim_set_icache_fill(e.0, 0), 0);
            if cost_model { assert_eq!(esp32sim_set_quantum(e.0, 128), 1); }
        }
    }
}

#[test]
fn cache_order_and_modes_are_validated_before_configuration() {
    let e = Emulator::new("atech14");
    // SAFETY: all calls exclusively borrow the live handle.
    unsafe {
        assert_eq!(esp32sim_set_approximate_jit_timing(e.0, 1, 512), 0);
        for mode in [3, 99, u32::MAX] {
            assert_eq!(esp32sim_set_approximate_jit_frontiers(e.0, mode), 1);
        }
        for mode in [4, 99, u32::MAX] {
            assert_eq!(esp32sim_set_approximate_jit_cache(e.0, 96, 40, mode), 1);
        }
        // Native builds cannot install the WASM inline probe. Failure must not install cache.
        #[cfg(not(all(target_arch = "wasm32", feature = "cache-inline")))]
        for mode in [2, 3] {
            assert_eq!(esp32sim_set_approximate_jit_cache(e.0, 96, 40, mode), 1);
        }
        assert_eq!(esp32sim_set_approximate_cache_contention(e.0, 1), 1);
        assert_eq!(esp32sim_set_approximate_cache_contention(e.0, 0), 1);
        assert_eq!(esp32sim_set_approximate_cache_fill_service(e.0, 160), 1);
        assert_eq!(esp32sim_set_approximate_flash_timing(e.0, 96, 160), 1);
        for mode in [0, 1] {
            assert_eq!(esp32sim_set_approximate_jit_cache(e.0, 96, 40, mode), 0);
            assert_eq!(esp32sim_set_approximate_cache_contention(e.0, 1), 0);
            assert_eq!(esp32sim_set_approximate_cache_fill_service(e.0, 95), 1);
            assert_eq!(esp32sim_set_approximate_cache_fill_service(e.0, 160), 0);
            assert_eq!(esp32sim_set_approximate_flash_timing(e.0, 96, 160), 0);
        }
    }
}

#[test]
fn preboot_prices_and_scheduler_cannot_be_detached() {
    let e = Emulator::new("atech14");
    // SAFETY: all calls exclusively borrow the live handle.
    unsafe {
        assert_eq!(esp32sim_set_quantum(e.0, 128), 0);
        assert_eq!(esp32sim_set_approximate_jit_timing(e.0, 1, 512), 0);
        assert_eq!(esp32sim_set_approximate_jit_cache(e.0, 96, 40, 0), 0);
        assert_eq!(esp32sim_set_approximate_pie_timing(e.0, 2), 0);
        assert_eq!(esp32sim_set_control_prices(e.0, 1), 0);
        assert_eq!(esp32sim_set_icache_fill(e.0, 32), 0);
        assert_eq!(esp32sim_set_approximate_jit_timing(e.0, 0, 512), 1);
        assert_eq!(esp32sim_set_approximate_jit_timing(e.0, 1, 0), 1);
        // Disabling frontiers or JIT does not detach the approximate scheduler.
        assert_eq!(esp32sim_set_approximate_jit_frontiers(e.0, 0), 0);
        esp32sim_set_jit(e.0, 0);
        assert_eq!(esp32sim_set_quantum(e.0, 128), 1);
        assert_eq!(esp32sim_set_approximate_timing(e.0), 1);
        assert_eq!(esp32sim_set_approximate_pie_timing(e.0, 3), 1);
        assert_eq!(esp32sim_set_approximate_jit_timing(e.0, 2, 256), 0);
    }
}

#[test]
fn wait_query_does_not_alias_invalid_cores() {
    for board in ["atech14", "esp32c3", "esp32c6"] {
        let e = Emulator::new(board);
        // SAFETY: all queries exclusively borrow the live handle.
        unsafe {
            for core in [2, 99, u32::MAX] {
                assert!(esp32sim_approximate_cache_wait(e.0, core).is_nan());
            }
            for core in [0, 1] {
                let wait = esp32sim_approximate_cache_wait(e.0, core);
                if board == "atech14" { assert_eq!(wait, 0.0); }
                else { assert!(wait.is_nan()); }
            }
        }
    }
}
