  (func $_RINvNtNtCs5F2i29bsihg_10xtensa_lx73jit6native9run_innerNtNtCs7lxvt5igX0z_7esp32s33bus6SocBusECsetaQYfaB9Ca_13esp32sim_wasm (;88;) (type 10) (param i32 i32 i32 i32 i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i64 i64 i64 i64 i64 i64 i64 i32 i32 i32 i32)
    global.get $__stack_pointer
    i32.const 464
    i32.sub
    local.tee 7
    global.set $__stack_pointer
    i32.const 0
    local.set 8
    block ;; label = @1
      i32.const 0
      i32.load8_u offset=1132321
      i32.eqz
      br_if 0 (;@1;)
      i32.const 0
      local.set 8
      local.get 3
      i32.load8_u offset=10045
      i32.const 1
      i32.ne
      br_if 0 (;@1;)
      i32.const 0
      local.set 8
      local.get 3
      i32.load offset=7764
      i32.const -1
      i32.eq
      br_if 0 (;@1;)
      block ;; label = @2
        local.get 3
        i32.load offset=7728
        local.tee 9
        i32.const 65536
        i32.eq
        br_if 0 (;@2;)
        i32.const 0
        local.set 8
        local.get 9
        i32.const 32768
        i32.ne
        br_if 1 (;@1;)
      end
      i32.const 0
      local.set 8
      local.get 3
      i32.load offset=7732
      i32.const 64
      i32.ne
      br_if 0 (;@1;)
      i32.const 0
      local.set 8
      local.get 3
      i32.load offset=7736
      i32.const 8
      i32.ne
      br_if 0 (;@1;)
      i32.const 0
      local.set 8
      local.get 3
      i32.load offset=7740
      br_if 0 (;@1;)
      local.get 7
      local.get 3
      i32.const 7696
      i32.add
      i32.store offset=40
      local.get 7
      local.get 3
      i32.load offset=7756
      i32.store offset=36
      i32.const 1
      local.set 8
    end
    local.get 7
    local.get 8
    i32.store offset=32
    local.get 7
    i64.const 0
    i64.store offset=56 align=4
    local.get 7
    i32.const 12
    i32.store offset=52
    local.get 7
    i32.const 13
    i32.store offset=48
    local.get 7
    i32.const 14
    i32.store offset=44
    local.get 7
    i64.const 0
    i64.store offset=64 align=4
    local.get 7
    i32.const 0
    i32.store offset=72
    local.get 7
    local.get 7
    i32.const 36
    i32.add
    i32.const 0
    local.get 8
    select
    i32.store offset=76
    block ;; label = @1
      block ;; label = @2
        block ;; label = @3
          block ;; label = @4
            block ;; label = @5
              block ;; label = @6
                block ;; label = @7
                  block ;; label = @8
                    block ;; label = @9
                      block ;; label = @10
                        block ;; label = @11
                          block ;; label = @12
                            block ;; label = @13
                              block ;; label = @14
                                block ;; label = @15
                                  local.get 1
                                  local.get 0
                                  i32.load offset=104
                                  local.tee 8
                                  i32.ge_u
                                  br_if 0 (;@15;)
                                  local.get 6
                                  i32.load offset=8
                                  i32.const 0
                                  local.get 6
                                  i32.load
                                  local.tee 9
                                  select
                                  local.set 10
                                  local.get 6
                                  i32.load offset=4
                                  i32.const 0
                                  local.get 9
                                  select
                                  local.set 11
                                  local.get 5
                                  br_if 9 (;@6;)
                                  local.get 2
                                  i32.load8_u offset=1145
                                  i32.const 1
                                  i32.and
                                  br_if 9 (;@6;)
                                  local.get 7
                                  i32.const 80
                                  i32.add
                                  local.get 0
                                  i32.load offset=100
                                  local.get 1
                                  i32.const 288
                                  i32.mul
                                  i32.add
                                  local.tee 12
                                  i32.const 96
                                  i32.add
                                  local.tee 13
                                  i32.const 112
                                  memory.copy
                                  local.get 7
                                  i64.load offset=144
                                  local.get 0
                                  i64.load offset=88
                                  i64.ne
                                  br_if 8 (;@7;)
                                  local.get 4
                                  local.get 7
                                  i32.load offset=168
                                  i32.lt_u
                                  br_if 8 (;@7;)
                                  local.get 7
                                  i64.load offset=152
                                  local.get 2
                                  i64.load offset=1264
                                  i64.and
                                  i64.const 0
                                  i64.ne
                                  br_if 8 (;@7;)
                                  block ;; label = @16
                                    local.get 2
                                    i32.load offset=1308
                                    i32.eqz
                                    br_if 0 (;@16;)
                                    local.get 2
                                    i32.load offset=1304
                                    local.get 7
                                    i32.load offset=172
                                    i32.sub
                                    local.get 7
                                    i32.load offset=176
                                    i32.le_u
                                    br_if 9 (;@7;)
                                  end
                                  local.get 7
                                  i32.load offset=180
                                  local.tee 8
                                  i32.const 9
                                  i32.ge_u
                                  br_if 1 (;@14;)
                                  local.get 2
                                  i32.const 1308
                                  i32.add
                                  local.set 14
                                  local.get 3
                                  i32.load offset=9960
                                  local.set 15
                                  local.get 3
                                  i32.load offset=9956
                                  local.set 16
                                  local.get 8
                                  i32.const 3
                                  i32.shl
                                  local.set 6
                                  local.get 7
                                  i32.const 80
                                  i32.add
                                  local.set 8
                                  block ;; label = @16
                                    loop ;; label = @17
                                      local.get 6
                                      i32.eqz
                                      br_if 1 (;@16;)
                                      local.get 8
                                      i32.const 4
                                      i32.add
                                      i32.load
                                      local.set 17
                                      i32.const 0
                                      local.set 9
                                      block ;; label = @18
                                        local.get 8
                                        i32.load
                                        local.tee 18
                                        local.get 15
                                        i32.ge_u
                                        br_if 0 (;@18;)
                                        local.get 16
                                        local.get 18
                                        i32.const 2
                                        i32.shl
                                        i32.add
                                        i32.load
                                        local.set 9
                                      end
                                      local.get 8
                                      i32.const 8
                                      i32.add
                                      local.set 8
                                      local.get 6
                                      i32.const -8
                                      i32.add
                                      local.set 6
                                      local.get 9
                                      local.get 17
                                      i32.ne
                                      br_if 10 (;@7;)
                                      br 0 (;@17;)
                                    end
                                  end
                                  block ;; label = @16
                                    local.get 2
                                    local.get 3
                                    local.get 7
                                    i32.const 44
                                    i32.add
                                    local.get 4
                                    i32.const 65535
                                    local.get 4
                                    i32.const 65535
                                    i32.lt_u
                                    select
                                    local.tee 6
                                    local.get 7
                                    i32.load offset=164
                                    local.get 11
                                    local.get 10
                                    local.get 7
                                    i32.load offset=160
                                    call_indirect (type 10)
                                    local.tee 8
                                    i32.const 458752
                                    i32.and
                                    i32.const 327680
                                    i32.ne
                                    br_if 0 (;@16;)
                                    local.get 7
                                    i32.const 0
                                    i32.store offset=404
                                    local.get 7
                                    local.get 4
                                    i32.store offset=400
                                    local.get 1
                                    local.get 0
                                    i32.load offset=104
                                    local.tee 8
                                    i32.ge_u
                                    br_if 3 (;@13;)
                                    local.get 0
                                    i32.load offset=100
                                    local.get 1
                                    i32.const 288
                                    i32.mul
                                    i32.add
                                    local.tee 8
                                    i32.load offset=248
                                    local.set 17
                                    local.get 7
                                    i32.const 16
                                    i32.add
                                    local.get 0
                                    local.get 1
                                    local.get 2
                                    call $_RNvNtNtCs5F2i29bsihg_10xtensa_lx73jit6native8loop_len
                                    local.get 7
                                    local.get 7
                                    i32.load offset=20
                                    local.tee 1
                                    i32.store offset=412
                                    local.get 7
                                    local.get 7
                                    i32.load offset=16
                                    local.tee 9
                                    i32.store offset=408
                                    local.get 7
                                    local.get 2
                                    i32.load offset=1308
                                    local.tee 5
                                    i32.store offset=416
                                    block ;; label = @17
                                      local.get 9
                                      i32.eqz
                                      br_if 0 (;@17;)
                                      local.get 7
                                      local.get 7
                                      i32.load offset=76
                                      i32.store offset=456
                                      local.get 7
                                      local.get 7
                                      i64.load offset=68 align=4
                                      i64.store offset=448
                                      local.get 7
                                      local.get 7
                                      i64.load offset=60 align=4
                                      i64.store offset=440
                                      local.get 7
                                      local.get 7
                                      i64.load offset=52 align=4
                                      i64.store offset=432
                                      local.get 7
                                      local.get 7
                                      i64.load offset=44 align=4
                                      i64.store offset=424
                                      local.get 8
                                      i32.load offset=276
                                      local.tee 0
                                      i32.eqz
                                      br_if 5 (;@12;)
                                      local.get 8
                                      i32.load offset=264
                                      local.tee 4
                                      i32.eqz
                                      br_if 6 (;@11;)
                                      local.get 8
                                      i32.load offset=260
                                      local.get 4
                                      i32.const 28
                                      i32.mul
                                      i32.add
                                      i32.const -9
                                      i32.add
                                      i32.load8_u
                                      local.set 4
                                      local.get 8
                                      i32.load offset=272
                                      local.get 0
                                      i32.const 2
                                      i32.shl
                                      i32.add
                                      i32.const -4
                                      i32.add
                                      i32.load
                                      local.set 18
                                      local.get 3
                                      local.get 8
                                      i32.load offset=232
                                      call $_RNvXs1_NtCs7lxvt5igX0z_7esp32s33busNtB5_6SocBusNtNtCs3QQYunS8OH3_8emu_core3bus3Bus9code_page
                                      local.set 0
                                      local.get 3
                                      local.get 18
                                      local.get 4
                                      i32.add
                                      i32.const -1
                                      i32.add
                                      call $_RNvXs1_NtCs7lxvt5igX0z_7esp32s33busNtB5_6SocBusNtNtCs3QQYunS8OH3_8emu_core3bus3Bus9code_page
                                      local.set 4
                                      local.get 0
                                      local.get 3
                                      i32.load offset=9960
                                      local.tee 18
                                      i32.ge_u
                                      br_if 8 (;@9;)
                                      local.get 4
                                      local.get 18
                                      i32.ge_u
                                      br_if 8 (;@9;)
                                      local.get 7
                                      local.get 2
                                      i32.load offset=1304
                                      i32.store offset=436
                                      local.get 7
                                      local.get 3
                                      i32.load offset=9956
                                      local.tee 18
                                      local.get 0
                                      i32.const 2
                                      i32.shl
                                      i32.add
                                      local.tee 0
                                      i32.store offset=440
                                      local.get 7
                                      local.get 18
                                      local.get 4
                                      i32.const 2
                                      i32.shl
                                      i32.add
                                      local.tee 4
                                      i32.store offset=444
                                      local.get 4
                                      i32.load
                                      local.set 4
                                      local.get 7
                                      local.get 0
                                      i32.load
                                      i32.store offset=448
                                      local.get 7
                                      local.get 4
                                      i32.store offset=452
                                      br 8 (;@9;)
                                    end
                                    local.get 2
                                    local.get 3
                                    local.get 7
                                    i32.const 44
                                    i32.add
                                    local.get 6
                                    i32.const 0
                                    local.get 11
                                    local.get 10
                                    local.get 17
                                    call_indirect (type 10)
                                    local.set 0
                                    br 8 (;@8;)
                                  end
                                  local.get 8
                                  i32.const 19
                                  i32.shr_u
                                  local.tee 0
                                  local.get 7
                                  i32.load offset=184
                                  i32.ge_u
                                  br_if 5 (;@10;)
                                  local.get 3
                                  local.get 7
                                  i32.load offset=188
                                  local.get 0
                                  i32.const 2
                                  i32.shl
                                  i32.add
                                  i32.load
                                  i32.store offset=6240
                                  local.get 8
                                  i32.const 524287
                                  i32.and
                                  local.set 0
                                  br 10 (;@5;)
                                end
                                local.get 1
                                local.get 8
                                i32.const 1061208
                                call $_RNvNtCsknUcikIyyBm_4core9panicking18panic_bounds_check
                                unreachable
                              end
                              i32.const 0
                              local.get 8
                              i32.const 8
                              i32.const 1061288
                              call $_RNvNtNtCsknUcikIyyBm_4core5slice5index16slice_index_fail
                              unreachable
                            end
                            local.get 1
                            local.get 8
                            i32.const 1061028
                            call $_RNvNtCsknUcikIyyBm_4core9panicking18panic_bounds_check
                            unreachable
                          end
                          i32.const 1061044
                          call $_RNvNtCsknUcikIyyBm_4core6option13unwrap_failed
                          unreachable
                        end
                        i32.const 1061060
                        call $_RNvNtCsknUcikIyyBm_4core6option13unwrap_failed
                        unreachable
                      end
                      i32.const 1061224
                      i32.const 45
                      i32.const 1061272
                      call $_RNvNtCsknUcikIyyBm_4core9panicking5panic
                      unreachable
                    end
                    local.get 2
                    local.get 3
                    local.get 7
                    i32.const 424
                    i32.add
                    local.get 6
                    i32.const 0
                    local.get 11
                    local.get 10
                    local.get 17
                    call_indirect (type 10)
                    local.set 0
                  end
                  local.get 7
                  local.get 0
                  i32.store offset=420
                  local.get 7
                  local.get 0
                  i32.const 65535
                  i32.and
                  local.tee 6
                  i32.store offset=460
                  block ;; label = @8
                    block ;; label = @9
                      block ;; label = @10
                        local.get 8
                        i32.load offset=276
                        local.tee 2
                        i32.eqz
                        br_if 0 (;@10;)
                        local.get 8
                        i32.load offset=264
                        i32.eqz
                        br_if 1 (;@9;)
                        block ;; label = @11
                          block ;; label = @12
                            block ;; label = @13
                              block ;; label = @14
                                local.get 9
                                i32.const 1
                                i32.ne
                                br_if 0 (;@14;)
                                local.get 6
                                local.get 14
                                i32.load
                                local.get 5
                                i32.sub
                                local.get 1
                                i32.mul
                                i32.add
                                local.set 9
                                local.get 6
                                br_if 1 (;@13;)
                                local.get 9
                                local.set 6
                                br 3 (;@11;)
                              end
                              local.get 6
                              local.set 1
                              local.get 6
                              br_if 1 (;@12;)
                              i32.const 0
                              local.set 6
                              br 2 (;@11;)
                            end
                            local.get 9
                            local.get 1
                            local.get 9
                            select
                            local.set 1
                            local.get 9
                            local.set 6
                          end
                          local.get 1
                          i32.const -1
                          i32.add
                          local.tee 9
                          local.get 2
                          i32.ge_u
                          br_if 3 (;@8;)
                          local.get 3
                          local.get 8
                          i32.load offset=272
                          local.get 9
                          i32.const 2
                          i32.shl
                          i32.add
                          i32.load
                          i32.store offset=6240
                        end
                        local.get 6
                        i32.const 19
                        i32.shl
                        i32.const 0
                        local.get 0
                        i32.const -65536
                        i32.and
                        i32.const 196608
                        i32.eq
                        select
                        local.get 0
                        i32.or
                        local.set 0
                        br 5 (;@5;)
                      end
                      i32.const 1061076
                      call $_RNvNtCsknUcikIyyBm_4core6option13unwrap_failed
                      unreachable
                    end
                    i32.const 1061092
                    call $_RNvNtCsknUcikIyyBm_4core6option13unwrap_failed
                    unreachable
                  end
                  local.get 7
                  local.get 14
                  i32.store offset=428
                  local.get 7
                  local.get 8
                  i32.store offset=424
                  local.get 7
                  local.get 7
                  i32.const 420
                  i32.add
                  i32.store offset=452
                  local.get 7
                  local.get 7
                  i32.const 416
                  i32.add
                  i32.store offset=448
                  local.get 7
                  local.get 7
                  i32.const 408
                  i32.add
                  i32.store offset=444
                  local.get 7
                  local.get 7
                  i32.const 400
                  i32.add
                  i32.store offset=440
                  local.get 7
                  local.get 7
                  i32.const 460
                  i32.add
                  i32.store offset=436
                  local.get 7
                  local.get 7
                  i32.const 404
                  i32.add
                  i32.store offset=432
                  local.get 7
                  i32.const 424
                  i32.add
                  call $_RNCINvNtNtCs5F2i29bsihg_10xtensa_lx73jit6native14run_block_bodyNtNtCs7lxvt5igX0z_7esp32s33bus6SocBusEs_0CsetaQYfaB9Ca_13esp32sim_wasm
                  unreachable
                end
                block ;; label = @7
                  block ;; label = @8
                    block ;; label = @9
                      block ;; label = @10
                        block ;; label = @11
                          block ;; label = @12
                            local.get 12
                            i32.load
                            local.tee 8
                            i32.const 2147483647
                            i32.ge_u
                            br_if 0 (;@12;)
                            local.get 12
                            local.get 8
                            i32.store
                            i32.const 0
                            local.set 19
                            block ;; label = @13
                              local.get 12
                              i32.load offset=16
                              i32.const -1
                              i32.eq
                              br_if 0 (;@13;)
                              local.get 1
                              local.set 16
                              br 4 (;@9;)
                            end
                            local.get 12
                            i32.load offset=216
                            local.tee 16
                            i32.const -1
                            i32.eq
                            br_if 2 (;@10;)
                            local.get 16
                            local.get 0
                            i32.load offset=104
                            i32.ge_u
                            br_if 2 (;@10;)
                            local.get 0
                            i32.load offset=100
                            local.get 16
                            i32.const 288
                            i32.mul
                            i32.add
                            local.tee 8
                            i32.load
                            local.tee 6
                            i32.const 2147483647
                            i32.ge_u
                            br_if 1 (;@11;)
                            local.get 12
                            i32.load offset=220
                            local.set 19
                            local.get 8
                            local.get 6
                            i32.const 1
                            i32.add
                            i32.store
                            block ;; label = @13
                              block ;; label = @14
                                local.get 8
                                i32.load offset=16
                                i32.const -1
                                i32.eq
                                br_if 0 (;@14;)
                                local.get 19
                                local.get 8
                                i32.load offset=24
                                i32.lt_u
                                br_if 1 (;@13;)
                              end
                              local.get 8
                              local.get 6
                              i32.store
                              br 3 (;@10;)
                            end
                            local.get 8
                            i32.load offset=20
                            local.get 19
                            i32.const 4
                            i32.shl
                            i32.add
                            i32.load offset=12
                            local.set 9
                            local.get 12
                            i32.load offset=232
                            local.set 17
                            local.get 8
                            local.get 6
                            i32.store
                            local.get 9
                            local.get 17
                            i32.ne
                            br_if 2 (;@10;)
                            br 5 (;@7;)
                          end
                          i32.const 1061304
                          call $_RNvNtCsknUcikIyyBm_4core4cell30panic_already_mutably_borrowed
                          unreachable
                        end
                        i32.const 1061760
                        call $_RNvNtCsknUcikIyyBm_4core4cell30panic_already_mutably_borrowed
                        unreachable
                      end
                      block ;; label = @10
                        block ;; label = @11
                          block ;; label = @12
                            local.get 12
                            i64.load offset=224
                            local.get 0
                            i64.load offset=80
                            local.tee 20
                            i64.eq
                            br_if 0 (;@12;)
                            local.get 0
                            i32.load offset=40
                            local.tee 8
                            i32.const 2147483647
                            i32.ge_u
                            br_if 1 (;@11;)
                            local.get 0
                            local.get 8
                            i32.const 1
                            i32.add
                            i32.store offset=40
                            block ;; label = @13
                              local.get 0
                              i32.load offset=60
                              i32.eqz
                              br_if 0 (;@13;)
                              local.get 0
                              i32.load offset=52
                              local.tee 18
                              local.get 0
                              i64.load offset=72
                              local.tee 21
                              local.get 12
                              i32.load offset=232
                              local.tee 15
                              i64.extend_i32_u
                              local.tee 22
                              i64.xor
                              i64.const 8098989879002948979
                              i64.xor
                              local.tee 23
                              i64.const 16
                              i64.rotl
                              local.get 23
                              local.get 0
                              i64.load offset=64
                              local.tee 24
                              i64.const 7816392313619706465
                              i64.xor
                              i64.add
                              local.tee 23
                              i64.xor
                              local.tee 25
                              local.get 21
                              i64.const 7237128888997146477
                              i64.xor
                              local.tee 21
                              local.get 24
                              i64.const 8317987319222330741
                              i64.xor
                              i64.add
                              local.tee 24
                              i64.const 32
                              i64.rotl
                              i64.add
                              local.tee 26
                              local.get 22
                              i64.const 288230376151711744
                              i64.or
                              i64.xor
                              local.get 21
                              i64.const 13
                              i64.rotl
                              local.get 24
                              i64.xor
                              local.tee 21
                              local.get 23
                              i64.add
                              local.tee 22
                              local.get 21
                              i64.const 17
                              i64.rotl
                              i64.xor
                              local.tee 21
                              i64.add
                              local.tee 23
                              local.get 21
                              i64.const 13
                              i64.rotl
                              i64.xor
                              local.tee 21
                              local.get 22
                              i64.const 32
                              i64.rotl
                              i64.const 255
                              i64.xor
                              local.get 25
                              i64.const 21
                              i64.rotl
                              local.get 26
                              i64.xor
                              local.tee 22
                              i64.add
                              local.tee 24
                              i64.add
                              local.tee 25
                              local.get 21
                              i64.const 17
                              i64.rotl
                              i64.xor
                              local.tee 21
                              i64.const 13
                              i64.rotl
                              local.get 21
                              local.get 24
                              local.get 22
                              i64.const 16
                              i64.rotl
                              i64.xor
                              local.tee 22
                              local.get 23
                              i64.const 32
                              i64.rotl
                              i64.add
                              local.tee 23
                              i64.add
                              local.tee 21
                              i64.xor
                              local.tee 24
                              i64.const 17
                              i64.rotl
                              local.get 24
                              local.get 22
                              i64.const 21
                              i64.rotl
                              local.get 23
                              i64.xor
                              local.tee 22
                              local.get 25
                              i64.const 32
                              i64.rotl
                              i64.add
                              local.tee 23
                              i64.add
                              local.tee 24
                              i64.xor
                              local.tee 25
                              i64.const 13
                              i64.rotl
                              local.get 25
                              local.get 22
                              i64.const 16
                              i64.rotl
                              local.get 23
                              i64.xor
                              local.tee 22
                              local.get 21
                              i64.const 32
                              i64.rotl
                              i64.add
                              local.tee 21
                              i64.add
                              i64.xor
                              local.tee 23
                              i64.const 17
                              i64.rotl
                              local.get 22
                              i64.const 21
                              i64.rotl
                              local.get 21
                              i64.xor
                              local.tee 21
                              i64.const 16
                              i64.rotl
                              local.get 21
                              local.get 24
                              i64.const 32
                              i64.rotl
                              i64.add
                              local.tee 21
                              i64.xor
                              i64.const 21
                              i64.rotl
                              i64.xor
                              local.get 23
                              local.get 21
                              i64.add
                              local.tee 21
                              i64.const 32
                              i64.shr_u
                              i64.xor
                              local.get 21
                              i64.xor
                              local.tee 21
                              i32.wrap_i64
                              i32.and
                              local.set 6
                              local.get 21
                              i64.const 25
                              i64.shr_u
                              i64.const 127
                              i64.and
                              i64.const 72340172838076673
                              i64.mul
                              local.set 22
                              local.get 0
                              i32.load offset=48
                              local.set 9
                              i32.const 0
                              local.set 16
                              block ;; label = @14
                                loop ;; label = @15
                                  block ;; label = @16
                                    local.get 9
                                    local.get 6
                                    i32.add
                                    i64.load align=1
                                    local.tee 23
                                    local.get 22
                                    i64.xor
                                    local.tee 21
                                    i64.const -1
                                    i64.xor
                                    local.get 21
                                    i64.const -72340172838076673
                                    i64.add
                                    i64.and
                                    i64.const -9187201950435737472
                                    i64.and
                                    local.tee 21
                                    i64.eqz
                                    br_if 0 (;@16;)
                                    loop ;; label = @17
                                      local.get 15
                                      local.get 9
                                      i32.const 0
                                      local.get 21
                                      i64.ctz
                                      i32.wrap_i64
                                      i32.const 3
                                      i32.shr_u
                                      local.get 6
                                      i32.add
                                      local.get 18
                                      i32.and
                                      i32.sub
                                      i32.const 12
                                      i32.mul
                                      i32.add
                                      local.tee 17
                                      i32.const -12
                                      i32.add
                                      i32.load
                                      i32.eq
                                      br_if 3 (;@14;)
                                      local.get 21
                                      i64.const -1
                                      i64.add
                                      local.get 21
                                      i64.and
                                      local.tee 21
                                      i64.eqz
                                      i32.eqz
                                      br_if 0 (;@17;)
                                    end
                                  end
                                  local.get 23
                                  local.get 23
                                  i64.const 1
                                  i64.shl
                                  i64.and
                                  i64.const -9187201950435737472
                                  i64.and
                                  i64.eqz
                                  i32.eqz
                                  br_if 2 (;@13;)
                                  local.get 6
                                  local.get 16
                                  i32.const 8
                                  i32.add
                                  local.tee 16
                                  i32.add
                                  local.get 18
                                  i32.and
                                  local.set 6
                                  br 0 (;@15;)
                                end
                              end
                              local.get 17
                              i32.const -8
                              i32.add
                              i32.load
                              local.set 16
                              local.get 17
                              i32.const -4
                              i32.add
                              i32.load
                              local.set 19
                              local.get 0
                              local.get 8
                              i32.store offset=40
                              local.get 12
                              local.get 19
                              i32.store offset=220
                              local.get 12
                              local.get 16
                              i32.store offset=216
                              br 4 (;@9;)
                            end
                            local.get 0
                            local.get 8
                            i32.store offset=40
                            local.get 12
                            local.get 20
                            i64.store offset=224
                          end
                          local.get 12
                          i32.load8_u offset=280
                          local.tee 8
                          i32.const 7
                          i32.gt_u
                          br_if 3 (;@8;)
                          local.get 12
                          local.get 8
                          i32.const 1
                          i32.add
                          i32.store8 offset=280
                          local.get 7
                          i32.const 280
                          i32.add
                          local.get 2
                          i64.load offset=1264
                          local.get 2
                          i32.load8_u offset=1517
                          local.get 3
                          local.get 12
                          i32.load offset=232
                          local.get 12
                          i32.load offset=260
                          local.get 12
                          i32.load offset=264
                          local.get 12
                          i32.load8_u offset=281
                          call $_RINvNtNtNtNtCs5F2i29bsihg_10xtensa_lx73jit6native7emitter6region4formNtNtCs7lxvt5igX0z_7esp32s33bus6SocBusECsetaQYfaB9Ca_13esp32sim_wasm
                          block ;; label = @12
                            block ;; label = @13
                              block ;; label = @14
                                local.get 7
                                i32.load offset=288
                                i32.const -1
                                i32.eq
                                br_if 0 (;@14;)
                                local.get 7
                                i32.const 192
                                i32.add
                                local.get 12
                                i32.load8_u offset=281
                                local.get 7
                                i32.const 280
                                i32.add
                                call $_RNCINvNtNtCs5F2i29bsihg_10xtensa_lx73jit6native9run_innerNtNtCs7lxvt5igX0z_7esp32s33bus6SocBusEs2_0CsetaQYfaB9Ca_13esp32sim_wasm
                                local.get 7
                                i32.load offset=200
                                i32.const -1
                                i32.eq
                                br_if 4 (;@10;)
                                local.get 0
                                local.get 0
                                i64.load offset=80
                                i64.const 1
                                i64.add
                                i64.store offset=80
                                local.get 0
                                i32.load offset=40
                                br_if 1 (;@13;)
                                local.get 0
                                i32.const -1
                                i32.store offset=40
                                local.get 7
                                i32.load offset=208
                                local.tee 8
                                br_if 2 (;@12;)
                                local.get 0
                                i32.const 0
                                i32.store offset=40
                                br 4 (;@10;)
                              end
                              local.get 7
                              i32.const -1
                              i32.store offset=200
                              br 3 (;@10;)
                            end
                            i32.const 1061336
                            call $_RNvNtCsknUcikIyyBm_4core4cell22panic_already_borrowed
                            unreachable
                          end
                          local.get 7
                          i32.load offset=204
                          local.tee 6
                          local.get 8
                          i32.const 4
                          i32.shl
                          i32.add
                          local.set 27
                          local.get 0
                          i32.const 64
                          i32.add
                          local.set 28
                          local.get 0
                          i32.const 48
                          i32.add
                          local.set 29
                          i32.const 0
                          local.set 18
                          loop ;; label = @12
                            local.get 0
                            i64.load offset=72
                            local.tee 20
                            local.get 6
                            i32.load offset=12
                            local.tee 15
                            i64.extend_i32_u
                            local.tee 21
                            i64.xor
                            i64.const 8098989879002948979
                            i64.xor
                            local.tee 22
                            i64.const 16
                            i64.rotl
                            local.get 22
                            local.get 0
                            i64.load offset=64
                            local.tee 23
                            i64.const 7816392313619706465
                            i64.xor
                            i64.add
                            local.tee 22
                            i64.xor
                            local.tee 24
                            local.get 20
                            i64.const 7237128888997146477
                            i64.xor
                            local.tee 20
                            local.get 23
                            i64.const 8317987319222330741
                            i64.xor
                            i64.add
                            local.tee 23
                            i64.const 32
                            i64.rotl
                            i64.add
                            local.tee 25
                            local.get 21
                            i64.const 288230376151711744
                            i64.or
                            i64.xor
                            local.get 20
                            i64.const 13
                            i64.rotl
                            local.get 23
                            i64.xor
                            local.tee 20
                            local.get 22
                            i64.add
                            local.tee 21
                            local.get 20
                            i64.const 17
                            i64.rotl
                            i64.xor
                            local.tee 20
                            i64.add
                            local.tee 22
                            local.get 20
                            i64.const 13
                            i64.rotl
                            i64.xor
                            local.tee 20
                            local.get 21
                            i64.const 32
                            i64.rotl
                            i64.const 255
                            i64.xor
                            local.get 24
                            i64.const 21
                            i64.rotl
                            local.get 25
                            i64.xor
                            local.tee 21
                            i64.add
                            local.tee 23
                            i64.add
                            local.tee 24
                            local.get 20
                            i64.const 17
                            i64.rotl
                            i64.xor
                            local.tee 20
                            i64.const 13
                            i64.rotl
                            local.get 20
                            local.get 23
                            local.get 21
                            i64.const 16
                            i64.rotl
                            i64.xor
                            local.tee 21
                            local.get 22
                            i64.const 32
                            i64.rotl
                            i64.add
                            local.tee 22
                            i64.add
                            local.tee 20
                            i64.xor
                            local.tee 23
                            i64.const 17
                            i64.rotl
                            local.get 23
                            local.get 21
                            i64.const 21
                            i64.rotl
                            local.get 22
                            i64.xor
                            local.tee 21
                            local.get 24
                            i64.const 32
                            i64.rotl
                            i64.add
                            local.tee 22
                            i64.add
                            local.tee 23
                            i64.xor
                            local.tee 24
                            i64.const 13
                            i64.rotl
                            local.get 24
                            local.get 21
                            i64.const 16
                            i64.rotl
                            local.get 22
                            i64.xor
                            local.tee 21
                            local.get 20
                            i64.const 32
                            i64.rotl
                            i64.add
                            local.tee 20
                            i64.add
                            i64.xor
                            local.tee 22
                            i64.const 17
                            i64.rotl
                            local.get 21
                            i64.const 21
                            i64.rotl
                            local.get 20
                            i64.xor
                            local.tee 20
                            i64.const 16
                            i64.rotl
                            local.get 20
                            local.get 23
                            i64.const 32
                            i64.rotl
                            i64.add
                            local.tee 20
                            i64.xor
                            i64.const 21
                            i64.rotl
                            i64.xor
                            local.get 22
                            local.get 20
                            i64.add
                            local.tee 20
                            i64.const 32
                            i64.shr_u
                            i64.xor
                            local.get 20
                            i64.xor
                            local.tee 20
                            i64.const 25
                            i64.shr_u
                            local.tee 23
                            i64.const 127
                            i64.and
                            i64.const 72340172838076673
                            i64.mul
                            local.set 21
                            local.get 18
                            local.tee 14
                            i32.const 1
                            i32.add
                            local.set 18
                            local.get 6
                            i32.const 16
                            i32.add
                            local.set 6
                            local.get 0
                            i32.load offset=48
                            local.set 8
                            i32.const 0
                            local.set 19
                            local.get 0
                            i32.load offset=52
                            local.tee 17
                            local.get 20
                            i32.wrap_i64
                            local.tee 30
                            i32.and
                            local.tee 16
                            local.set 9
                            block ;; label = @13
                              loop ;; label = @14
                                block ;; label = @15
                                  local.get 8
                                  local.get 9
                                  i32.add
                                  i64.load align=1
                                  local.tee 22
                                  local.get 21
                                  i64.xor
                                  local.tee 20
                                  i64.const -1
                                  i64.xor
                                  local.get 20
                                  i64.const -72340172838076673
                                  i64.add
                                  i64.and
                                  i64.const -9187201950435737472
                                  i64.and
                                  local.tee 20
                                  i64.eqz
                                  br_if 0 (;@15;)
                                  loop ;; label = @16
                                    local.get 8
                                    i32.const 0
                                    local.get 20
                                    i64.ctz
                                    i32.wrap_i64
                                    i32.const 3
                                    i32.shr_u
                                    local.get 9
                                    i32.add
                                    local.get 17
                                    i32.and
                                    i32.sub
                                    i32.const 12
                                    i32.mul
                                    i32.add
                                    i32.const -12
                                    i32.add
                                    i32.load
                                    local.get 15
                                    i32.eq
                                    br_if 3 (;@13;)
                                    local.get 20
                                    i64.const -1
                                    i64.add
                                    local.get 20
                                    i64.and
                                    local.tee 20
                                    i64.eqz
                                    i32.eqz
                                    br_if 0 (;@16;)
                                  end
                                end
                                block ;; label = @15
                                  local.get 22
                                  local.get 22
                                  i64.const 1
                                  i64.shl
                                  i64.and
                                  i64.const -9187201950435737472
                                  i64.and
                                  i64.eqz
                                  i32.eqz
                                  br_if 0 (;@15;)
                                  local.get 9
                                  local.get 19
                                  i32.const 8
                                  i32.add
                                  local.tee 19
                                  i32.add
                                  local.get 17
                                  i32.and
                                  local.set 9
                                  br 1 (;@14;)
                                end
                              end
                              block ;; label = @14
                                local.get 0
                                i32.load offset=56
                                br_if 0 (;@14;)
                                local.get 7
                                i32.const 24
                                i32.add
                                local.get 29
                                i32.const 1
                                local.get 28
                                i32.const 1
                                call $_RINvMs6_NtCslWL5Yxw0Iyw_9hashbrown3rawINtB6_8RawTableTmTjmEEE14reserve_rehashNCINvNtB8_3map11make_hashermBR_NtNtNtCs7L774UPzF0f_3std4hash6random11RandomStateE0ECs5F2i29bsihg_10xtensa_lx7
                                local.get 0
                                i32.load offset=52
                                local.tee 17
                                local.get 30
                                i32.and
                                local.set 16
                                local.get 0
                                i32.load offset=48
                                local.set 8
                              end
                              block ;; label = @14
                                local.get 8
                                local.get 16
                                i32.add
                                i64.load align=1
                                i64.const -9187201950435737472
                                i64.and
                                local.tee 20
                                i64.const 0
                                i64.ne
                                br_if 0 (;@14;)
                                i32.const 8
                                local.set 9
                                loop ;; label = @15
                                  local.get 16
                                  local.get 9
                                  i32.add
                                  local.set 16
                                  local.get 9
                                  i32.const 8
                                  i32.add
                                  local.set 9
                                  local.get 8
                                  local.get 16
                                  local.get 17
                                  i32.and
                                  local.tee 16
                                  i32.add
                                  i64.load align=1
                                  i64.const -9187201950435737472
                                  i64.and
                                  local.tee 20
                                  i64.eqz
                                  br_if 0 (;@15;)
                                end
                              end
                              block ;; label = @14
                                local.get 8
                                local.get 20
                                i64.ctz
                                i32.wrap_i64
                                i32.const 3
                                i32.shr_u
                                local.get 16
                                i32.add
                                local.get 17
                                i32.and
                                local.tee 9
                                i32.add
                                i32.load8_s
                                local.tee 16
                                i32.const 0
                                i32.lt_s
                                br_if 0 (;@14;)
                                local.get 8
                                local.get 8
                                i64.load
                                i64.const -9187201950435737472
                                i64.and
                                i64.ctz
                                i32.wrap_i64
                                i32.const 3
                                i32.shr_u
                                local.tee 9
                                i32.add
                                i32.load8_u
                                local.set 16
                              end
                              local.get 8
                              local.get 9
                              i32.add
                              local.get 23
                              i32.wrap_i64
                              i32.const 127
                              i32.and
                              local.tee 19
                              i32.store8
                              local.get 8
                              local.get 9
                              i32.const -8
                              i32.add
                              local.get 17
                              i32.and
                              i32.add
                              i32.const 8
                              i32.add
                              local.get 19
                              i32.store8
                              local.get 0
                              local.get 0
                              i32.load offset=56
                              local.get 16
                              i32.const 1
                              i32.and
                              i32.sub
                              i32.store offset=56
                              local.get 0
                              local.get 0
                              i32.load offset=60
                              i32.const 1
                              i32.add
                              i32.store offset=60
                              local.get 8
                              i32.const 0
                              local.get 9
                              i32.sub
                              i32.const 12
                              i32.mul
                              i32.add
                              local.tee 8
                              i32.const -4
                              i32.add
                              local.get 14
                              i32.store
                              local.get 8
                              i32.const -8
                              i32.add
                              local.get 1
                              i32.store
                              local.get 8
                              i32.const -12
                              i32.add
                              local.get 15
                              i32.store
                            end
                            local.get 6
                            local.get 27
                            i32.ne
                            br_if 0 (;@12;)
                          end
                          local.get 0
                          local.get 0
                          i32.load offset=40
                          i32.const 1
                          i32.add
                          i32.store offset=40
                          br 1 (;@10;)
                        end
                        i32.const 1061320
                        call $_RNvNtCsknUcikIyyBm_4core4cell30panic_already_mutably_borrowed
                        unreachable
                      end
                      block ;; label = @10
                        local.get 12
                        i32.load
                        br_if 0 (;@10;)
                        local.get 12
                        i32.const -1
                        i32.store
                        local.get 12
                        i32.const 8
                        i32.add
                        local.tee 8
                        call $_RINvNtCsknUcikIyyBm_4core3ptr9drop_glueINtNtB4_6option6OptionNtNtNtCs5F2i29bsihg_10xtensa_lx73jit6native6RegionEECsetaQYfaB9Ca_13esp32sim_wasm
                        local.get 8
                        local.get 7
                        i32.const 192
                        i32.add
                        i32.const 88
                        memory.copy
                        local.get 12
                        local.get 12
                        i32.load
                        i32.const 1
                        i32.add
                        i32.store
                        local.get 1
                        local.set 16
                        i32.const 0
                        local.set 19
                        br 1 (;@9;)
                      end
                      i32.const 1061352
                      call $_RNvNtCsknUcikIyyBm_4core4cell22panic_already_borrowed
                      unreachable
                    end
                    local.get 16
                    i32.const -1
                    i32.ne
                    br_if 1 (;@7;)
                  end
                  local.get 0
                  i32.load offset=104
                  local.set 8
                  br 1 (;@6;)
                end
                block ;; label = @7
                  block ;; label = @8
                    block ;; label = @9
                      block ;; label = @10
                        block ;; label = @11
                          local.get 16
                          local.get 0
                          i32.load offset=104
                          local.tee 8
                          i32.ge_u
                          br_if 0 (;@11;)
                          block ;; label = @12
                            local.get 0
                            i32.load offset=100
                            local.get 16
                            i32.const 288
                            i32.mul
                            i32.add
                            local.tee 14
                            i32.load
                            local.tee 30
                            i32.const 2147483647
                            i32.ge_u
                            br_if 0 (;@12;)
                            local.get 14
                            local.get 30
                            i32.const 1
                            i32.add
                            i32.store
                            local.get 14
                            i32.load offset=16
                            i32.const -1
                            i32.eq
                            br_if 4 (;@8;)
                            local.get 14
                            i32.load offset=48
                            local.tee 28
                            i32.const 3
                            i32.shl
                            local.set 6
                            local.get 3
                            i32.load offset=9960
                            local.set 15
                            local.get 3
                            i32.load offset=9956
                            local.set 27
                            local.get 14
                            i32.load offset=44
                            local.tee 29
                            local.set 8
                            block ;; label = @13
                              loop ;; label = @14
                                local.get 6
                                i32.eqz
                                br_if 1 (;@13;)
                                local.get 8
                                i32.const 4
                                i32.add
                                i32.load
                                local.set 17
                                i32.const 0
                                local.set 9
                                block ;; label = @15
                                  local.get 8
                                  i32.load
                                  local.tee 18
                                  local.get 15
                                  i32.ge_u
                                  br_if 0 (;@15;)
                                  local.get 27
                                  local.get 18
                                  i32.const 2
                                  i32.shl
                                  i32.add
                                  i32.load
                                  local.set 9
                                end
                                local.get 8
                                i32.const 8
                                i32.add
                                local.set 8
                                local.get 6
                                i32.const -8
                                i32.add
                                local.set 6
                                local.get 9
                                local.get 17
                                i32.eq
                                br_if 0 (;@14;)
                              end
                              local.get 14
                              local.get 30
                              i32.store
                              local.get 14
                              local.get 16
                              local.get 0
                              i32.const 40
                              i32.add
                              call $_RNvMs_NtNtCs5F2i29bsihg_10xtensa_lx73jit6nativeNtB4_5Block11drop_region
                              local.get 0
                              local.get 0
                              i64.load offset=88
                              i64.const 1
                              i64.add
                              i64.store offset=88
                              local.get 0
                              i32.load offset=104
                              local.set 8
                              br 7 (;@6;)
                            end
                            local.get 19
                            local.get 14
                            i32.load offset=72
                            local.tee 8
                            i32.ge_u
                            br_if 2 (;@10;)
                            local.get 4
                            local.get 14
                            i32.load offset=68
                            local.get 19
                            i32.const 2
                            i32.shl
                            i32.add
                            i32.load
                            local.tee 17
                            i32.lt_u
                            br_if 4 (;@8;)
                            local.get 14
                            i64.load offset=8
                            local.tee 20
                            local.get 2
                            i64.load offset=1264
                            i64.and
                            i64.const 0
                            i64.ne
                            br_if 4 (;@8;)
                            local.get 2
                            i32.load offset=1308
                            i32.eqz
                            br_if 3 (;@9;)
                            local.get 2
                            i32.load offset=1304
                            local.tee 18
                            local.get 14
                            i32.load offset=84
                            local.tee 8
                            i32.sub
                            local.get 14
                            i32.load offset=88
                            local.get 8
                            i32.sub
                            i32.gt_u
                            br_if 3 (;@9;)
                            local.get 14
                            i32.load offset=36
                            i32.const 3
                            i32.shl
                            local.set 6
                            local.get 2
                            i32.load offset=1300
                            local.set 15
                            local.get 14
                            i32.load offset=32
                            local.set 9
                            loop ;; label = @13
                              local.get 9
                              local.set 8
                              local.get 6
                              i32.eqz
                              br_if 5 (;@8;)
                              local.get 6
                              i32.const -8
                              i32.add
                              local.set 6
                              local.get 8
                              i32.const 8
                              i32.add
                              local.set 9
                              local.get 8
                              i32.load
                              local.get 18
                              i32.ne
                              br_if 0 (;@13;)
                              local.get 8
                              i32.const 4
                              i32.add
                              i32.load
                              local.get 15
                              i32.eq
                              br_if 4 (;@9;)
                              br 0 (;@13;)
                            end
                          end
                          i32.const 1061384
                          call $_RNvNtCsknUcikIyyBm_4core4cell30panic_already_mutably_borrowed
                          unreachable
                        end
                        local.get 16
                        local.get 8
                        i32.const 1061368
                        call $_RNvNtCsknUcikIyyBm_4core9panicking18panic_bounds_check
                        unreachable
                      end
                      local.get 19
                      local.get 8
                      i32.const 1061400
                      call $_RNvNtCsknUcikIyyBm_4core9panicking18panic_bounds_check
                      unreachable
                    end
                    local.get 14
                    i32.load offset=76
                    local.set 8
                    block ;; label = @9
                      local.get 28
                      i32.const 9
                      i32.ge_u
                      br_if 0 (;@9;)
                      local.get 7
                      i64.const 0
                      i64.store offset=392
                      local.get 7
                      i64.const 0
                      i64.store offset=384
                      local.get 7
                      i64.const 0
                      i64.store offset=376
                      local.get 7
                      i64.const 0
                      i64.store offset=368
                      local.get 7
                      i64.const 0
                      i64.store offset=360
                      local.get 7
                      i64.const 0
                      i64.store offset=352
                      local.get 7
                      i64.const 0
                      i64.store offset=344
                      local.get 7
                      i64.const 0
                      i64.store offset=336
                      block ;; label = @10
                        local.get 28
                        i32.const 3
                        i32.shl
                        local.tee 6
                        i32.eqz
                        br_if 0 (;@10;)
                        local.get 7
                        i32.const 336
                        i32.add
                        local.get 29
                        local.get 6
                        memory.copy
                      end
                      local.get 14
                      i64.load offset=56
                      local.set 21
                      local.get 14
                      i32.load offset=88
                      local.set 9
                      local.get 14
                      i32.load offset=84
                      local.set 6
                      local.get 0
                      i64.load offset=88
                      local.set 22
                      local.get 13
                      local.get 7
                      i64.load offset=336
                      i64.store
                      local.get 13
                      local.get 7
                      i64.load offset=344
                      i64.store offset=8
                      local.get 13
                      local.get 7
                      i64.load offset=352
                      i64.store offset=16
                      local.get 13
                      local.get 7
                      i64.load offset=360
                      i64.store offset=24
                      local.get 13
                      local.get 7
                      i64.load offset=368
                      i64.store offset=32
                      local.get 13
                      local.get 7
                      i64.load offset=376
                      i64.store offset=40
                      local.get 13
                      local.get 7
                      i64.load offset=384
                      i64.store offset=48
                      local.get 13
                      local.get 7
                      i64.load offset=392
                      i64.store offset=56
                      local.get 12
                      local.get 22
                      i64.store offset=160
                      local.get 12
                      local.get 20
                      i64.store offset=168
                      local.get 12
                      local.get 8
                      i32.store offset=176
                      local.get 12
                      local.get 19
                      i32.store offset=180
                      local.get 12
                      local.get 17
                      i32.store offset=184
                      local.get 12
                      local.get 6
                      i32.store offset=188
                      local.get 12
                      local.get 28
                      i32.store offset=196
                      local.get 12
                      local.get 9
                      local.get 6
                      i32.sub
                      i32.store offset=192
                      local.get 12
                      local.get 21
                      i64.const 32
                      i64.rotl
                      i64.store offset=200
                    end
                    local.get 7
                    local.get 2
                    local.get 3
                    local.get 7
                    i32.const 44
                    i32.add
                    local.get 4
                    i32.const 65535
                    local.get 4
                    i32.const 65535
                    i32.lt_u
                    select
                    local.get 19
                    local.get 11
                    local.get 10
                    local.get 8
                    call_indirect (type 10)
                    local.tee 8
                    i32.store offset=460
                    local.get 8
                    i32.const 458752
                    i32.and
                    i32.const 327680
                    i32.ne
                    br_if 1 (;@7;)
                    local.get 14
                    i32.load
                    i32.const -1
                    i32.add
                    local.set 30
                  end
                  local.get 14
                  local.get 30
                  i32.store
                  local.get 0
                  i32.load offset=104
                  local.set 8
                  br 1 (;@6;)
                end
                block ;; label = @7
                  local.get 8
                  i32.const 19
                  i32.shr_u
                  local.tee 0
                  local.get 14
                  i32.load offset=60
                  local.tee 6
                  i32.ge_u
                  br_if 0 (;@7;)
                  local.get 3
                  local.get 14
                  i32.load offset=56
                  local.get 0
                  i32.const 2
                  i32.shl
                  i32.add
                  i32.load
                  i32.store offset=6240
                  local.get 14
                  local.get 14
                  i32.load
                  i32.const -1
                  i32.add
                  i32.store
                  local.get 8
                  i32.const 524287
                  i32.and
                  local.set 0
                  br 2 (;@5;)
                end
                local.get 7
                local.get 6
                i32.store offset=408
                local.get 7
                i32.const 3
                i64.extend_i32_u
                i64.const 32
                i64.shl
                local.get 7
                i32.const 408
                i32.add
                i64.extend_i32_u
                i64.or
                i64.store offset=440
                local.get 7
                i32.const 6
                i64.extend_i32_u
                i64.const 32
                i64.shl
                local.tee 20
                local.get 7
                i32.const 460
                i32.add
                i64.extend_i32_u
                i64.or
                i64.store offset=432
                local.get 7
                local.get 20
                local.get 14
                i32.const 232
                i32.add
                i64.extend_i32_u
                i64.or
                i64.store offset=424
                i32.const 1061416
                local.get 7
                i32.const 424
                i32.add
                i32.const 1061452
                call $_RNvNtCsknUcikIyyBm_4core9panicking9panic_fmt
                unreachable
              end
              local.get 7
              local.get 5
              i32.store offset=416
              local.get 7
              local.get 4
              i32.store offset=404
              block ;; label = @6
                block ;; label = @7
                  block ;; label = @8
                    block ;; label = @9
                      block ;; label = @10
                        local.get 1
                        local.get 8
                        i32.ge_u
                        br_if 0 (;@10;)
                        local.get 0
                        i32.load offset=100
                        local.get 1
                        i32.const 288
                        i32.mul
                        i32.add
                        local.tee 8
                        i32.load offset=248
                        local.set 9
                        local.get 7
                        i32.const 8
                        i32.add
                        local.get 0
                        local.get 1
                        local.get 2
                        call $_RNvNtNtCs5F2i29bsihg_10xtensa_lx73jit6native8loop_len
                        local.get 7
                        local.get 7
                        i32.load offset=12
                        local.tee 1
                        i32.store offset=428
                        local.get 7
                        local.get 7
                        i32.load offset=8
                        local.tee 6
                        i32.store offset=424
                        local.get 7
                        local.get 2
                        i32.load offset=1308
                        local.tee 18
                        i32.store offset=420
                        block ;; label = @11
                          local.get 6
                          i32.eqz
                          br_if 0 (;@11;)
                          local.get 7
                          local.get 7
                          i32.load offset=76
                          i32.store offset=112
                          local.get 7
                          local.get 7
                          i64.load offset=68 align=4
                          i64.store offset=104
                          local.get 7
                          local.get 7
                          i64.load offset=60 align=4
                          i64.store offset=96
                          local.get 7
                          local.get 7
                          i64.load offset=52 align=4
                          i64.store offset=88
                          local.get 7
                          local.get 7
                          i64.load offset=44 align=4
                          i64.store offset=80
                          local.get 8
                          i32.load offset=276
                          local.tee 0
                          i32.eqz
                          br_if 2 (;@9;)
                          local.get 8
                          i32.load offset=264
                          local.tee 17
                          i32.eqz
                          br_if 3 (;@8;)
                          local.get 8
                          i32.load offset=260
                          local.get 17
                          i32.const 28
                          i32.mul
                          i32.add
                          i32.const -9
                          i32.add
                          i32.load8_u
                          local.set 17
                          local.get 8
                          i32.load offset=272
                          local.get 0
                          i32.const 2
                          i32.shl
                          i32.add
                          i32.const -4
                          i32.add
                          i32.load
                          local.set 12
                          local.get 3
                          local.get 8
                          i32.load offset=232
                          call $_RNvXs1_NtCs7lxvt5igX0z_7esp32s33busNtB5_6SocBusNtNtCs3QQYunS8OH3_8emu_core3bus3Bus9code_page
                          local.set 0
                          local.get 3
                          local.get 12
                          local.get 17
                          i32.add
                          i32.const -1
                          i32.add
                          call $_RNvXs1_NtCs7lxvt5igX0z_7esp32s33busNtB5_6SocBusNtNtCs3QQYunS8OH3_8emu_core3bus3Bus9code_page
                          local.set 17
                          local.get 0
                          local.get 3
                          i32.load offset=9960
                          local.tee 12
                          i32.ge_u
                          br_if 4 (;@7;)
                          local.get 17
                          local.get 12
                          i32.ge_u
                          br_if 4 (;@7;)
                          local.get 7
                          local.get 2
                          i32.load offset=1304
                          i32.store offset=92
                          local.get 7
                          local.get 3
                          i32.load offset=9956
                          local.tee 12
                          local.get 0
                          i32.const 2
                          i32.shl
                          i32.add
                          local.tee 0
                          i32.store offset=96
                          local.get 7
                          local.get 12
                          local.get 17
                          i32.const 2
                          i32.shl
                          i32.add
                          local.tee 17
                          i32.store offset=100
                          local.get 17
                          i32.load
                          local.set 17
                          local.get 7
                          local.get 0
                          i32.load
                          i32.store offset=104
                          local.get 7
                          local.get 17
                          i32.store offset=108
                          br 4 (;@7;)
                        end
                        local.get 2
                        local.get 3
                        local.get 7
                        i32.const 44
                        i32.add
                        local.get 4
                        i32.const 65535
                        local.get 4
                        i32.const 65535
                        i32.lt_u
                        select
                        local.get 5
                        local.get 11
                        local.get 10
                        local.get 9
                        call_indirect (type 10)
                        local.set 0
                        br 4 (;@6;)
                      end
                      local.get 1
                      local.get 8
                      i32.const 1061028
                      call $_RNvNtCsknUcikIyyBm_4core9panicking18panic_bounds_check
                      unreachable
                    end
                    i32.const 1061044
                    call $_RNvNtCsknUcikIyyBm_4core6option13unwrap_failed
                    unreachable
                  end
                  i32.const 1061060
                  call $_RNvNtCsknUcikIyyBm_4core6option13unwrap_failed
                  unreachable
                end
                local.get 2
                local.get 3
                local.get 7
                i32.const 80
                i32.add
                local.get 4
                i32.const 65535
                local.get 4
                i32.const 65535
                i32.lt_u
                select
                local.get 5
                local.get 11
                local.get 10
                local.get 9
                call_indirect (type 10)
                local.set 0
              end
              local.get 7
              local.get 0
              i32.store offset=460
              local.get 7
              local.get 0
              i32.const 65535
              i32.and
              local.tee 9
              i32.store offset=408
              local.get 8
              i32.load offset=276
              local.tee 17
              i32.eqz
              br_if 1 (;@4;)
              local.get 8
              i32.load offset=264
              i32.eqz
              br_if 2 (;@3;)
              local.get 2
              i32.const 1308
              i32.add
              local.set 2
              block ;; label = @6
                block ;; label = @7
                  block ;; label = @8
                    local.get 6
                    i32.const 1
                    i32.ne
                    br_if 0 (;@8;)
                    local.get 5
                    local.get 9
                    i32.add
                    local.get 2
                    i32.load
                    local.get 18
                    i32.sub
                    local.get 1
                    i32.mul
                    i32.add
                    local.set 6
                    local.get 9
                    i32.eqz
                    br_if 2 (;@6;)
                    local.get 6
                    local.get 1
                    local.get 6
                    select
                    local.set 9
                    br 1 (;@7;)
                  end
                  local.get 5
                  local.get 9
                  i32.add
                  local.set 6
                  local.get 9
                  i32.eqz
                  br_if 1 (;@6;)
                  local.get 6
                  local.set 9
                  local.get 6
                  i32.eqz
                  br_if 6 (;@1;)
                end
                local.get 9
                i32.const -1
                i32.add
                local.tee 9
                local.get 17
                i32.ge_u
                br_if 4 (;@2;)
                local.get 3
                local.get 8
                i32.load offset=272
                local.get 9
                i32.const 2
                i32.shl
                i32.add
                i32.load
                i32.store offset=6240
              end
              local.get 6
              i32.const 19
              i32.shl
              i32.const 0
              local.get 0
              i32.const -65536
              i32.and
              i32.const 196608
              i32.eq
              select
              local.get 0
              i32.or
              local.set 0
            end
            local.get 7
            i32.const 464
            i32.add
            global.set $__stack_pointer
            local.get 0
            return
          end
          i32.const 1061076
          call $_RNvNtCsknUcikIyyBm_4core6option13unwrap_failed
          unreachable
        end
        i32.const 1061092
        call $_RNvNtCsknUcikIyyBm_4core6option13unwrap_failed
        unreachable
      end
      local.get 7
      local.get 2
      i32.store offset=84
      local.get 7
      local.get 8
      i32.store offset=80
      local.get 7
      local.get 7
      i32.const 460
      i32.add
      i32.store offset=108
      local.get 7
      local.get 7
      i32.const 420
      i32.add
      i32.store offset=104
      local.get 7
      local.get 7
      i32.const 424
      i32.add
      i32.store offset=100
      local.get 7
      local.get 7
      i32.const 404
      i32.add
      i32.store offset=96
      local.get 7
      local.get 7
      i32.const 408
      i32.add
      i32.store offset=92
      local.get 7
      local.get 7
      i32.const 416
      i32.add
      i32.store offset=88
      local.get 7
      i32.const 80
      i32.add
      call $_RNCINvNtNtCs5F2i29bsihg_10xtensa_lx73jit6native14run_block_bodyNtNtCs7lxvt5igX0z_7esp32s33bus6SocBusEs_0CsetaQYfaB9Ca_13esp32sim_wasm
      unreachable
    end
    i32.const 1061108
    call $_RNvNtCsknUcikIyyBm_4core6option13unwrap_failed
    unreachable
  )
