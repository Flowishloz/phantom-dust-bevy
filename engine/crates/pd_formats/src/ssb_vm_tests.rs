//! Unit tests for the script VM, one per opcode family, each citing the address in
//! `FUN_1402aaa30` (PDUWP.exe, image base 0x140000000) the behaviour was read from.

use super::*;
use crate::ssb::{build_test_script, build_test_script_with_pool};

// --- instruction word builders (b1 = bits 8..15, s = bits 16..31) ---
fn w(op: u8, b1: u8, s: i16) -> u32 {
    op as u32 | (b1 as u32) << 8 | (s as u16 as u32) << 16
}
fn push(v: i16) -> u32 {
    w(op::PUSH_I16, 0, v)
}
/// native call `g<group>.f<func>` dropping `n` argument words (op 1)
fn native(group: u32, func: u32, n: u8) -> u32 {
    op::NATIVE as u32 | (n as u32) << 8 | func << 16 | group << 28
}
/// the test host's recorder: group 7 function 0 logs its first argument
fn emit() -> u32 {
    native(7, 0, 1)
}
const RET: u32 = 0x0000_0005;

#[derive(Default)]
struct Rec {
    out: Vec<i64>,
    calls: Vec<(u8, u16, Vec<i64>)>,
    answer: i64,
}
impl Natives for Rec {
    fn call(&mut self, g: u8, f: u16, a: &[i64], mem: &Memory) -> i64 {
        self.calls.push((g, f, a.to_vec()));
        if g == 7 && f == 0 {
            self.out.push(a[0]);
        }
        if g == 7 && f == 1 {
            // log a string argument as the sum of its bytes
            self.out.push(mem.read_cstr(a[0]).map_or(-1, |s| s.bytes().map(|c| c as i64).sum()));
        }
        self.answer
    }
}

fn run_with(code: &[u32], pool: &[u8]) -> (Vm, Rec, RunStatus) {
    let s = build_test_script_with_pool(code, pool);
    let mut vm = Vm::new(&s);
    let mut rec = Rec::default();
    assert!(vm.call_by_name("main"));
    let st = vm.run(&mut rec, 1000);
    (vm, rec, st)
}
fn run(code: &[u32]) -> (Vm, Rec, RunStatus) {
    run_with(code, &[])
}
/// The code pushes values and logs them; returns the logged values.
fn outs(code: &[u32]) -> Vec<i64> {
    let (_, r, st) = run(code);
    assert_eq!(st, RunStatus::Finished, "{code:08x?}");
    r.out
}

#[test]
fn push_immediates_sign_extend() {
    // op 0x0E 0x1402aae5e (movsx word), op 0x0F 0x1402aae77 (movsxd dword)
    let v = outs(&[push(-5), emit(), w(op::PUSH_I32, 0, 0), 0xFFFF_FFFE, emit(), w(op::PUSH_I32, 0, 0), 0x7FFF_FFFF, emit(), RET]);
    assert_eq!(v, vec![-5, -2, 0x7FFF_FFFF]);
}

#[test]
fn arithmetic_operand_order() {
    // After PUSH 10; PUSH 3 the top is 3 (a) and the second is 10 (b).
    // b1 = 0: b OP a; b1 != 0: a OP b (ops 0x12..0x16, e.g. ADD at 0x1402aaef6).
    let bin = |o: u8, b1: u8| outs(&[push(10), push(3), w(o, b1, 0), emit(), RET])[0];
    assert_eq!(bin(op::ADD, 0), 13);
    assert_eq!(bin(op::SUB, 0), 7);
    assert_eq!(bin(op::SUB, 1), -7);
    assert_eq!(bin(op::MUL, 0), 30);
    assert_eq!(bin(op::DIV, 0), 3);
    assert_eq!(bin(op::DIV, 1), 0);
    assert_eq!(bin(op::MOD, 0), 1);
    assert_eq!(bin(op::MOD, 1), 3);
    // C truncation toward zero
    assert_eq!(outs(&[push(-7), push(2), w(op::DIV, 0, 0), emit(), RET])[0], -3);
    assert_eq!(outs(&[push(-7), push(2), w(op::MOD, 0, 0), emit(), RET])[0], -1);
}

#[test]
fn divide_by_zero_faults() {
    // an x86 #DE in the program (idiv); the fault flag here
    let (vm, _, st) = run(&[push(1), push(0), w(op::DIV, 0, 0), RET]);
    assert_eq!(st, RunStatus::Faulted);
    assert_eq!(vm.flags & FLAG_FAULT, FLAG_FAULT);
}

#[test]
fn comparisons() {
    // op 0x17: left = the second word (first pushed) when b1 == 0, the top word otherwise
    let cmp = |a: i16, b: i16, s: i16, b1: u8| outs(&[push(a), push(b), w(op::CMP, b1, s), emit(), RET])[0];
    for (s, name) in [(0, "EQ"), (1, "NE"), (2, "GT"), (3, "LT"), (4, "GE"), (5, "LE")] {
        let f = |l: i16, r: i16| -> i64 {
            (match name {
                "EQ" => l == r,
                "NE" => l != r,
                "GT" => l > r,
                "LT" => l < r,
                "GE" => l >= r,
                _ => l <= r,
            }) as i64
        };
        for (a, b) in [(1, 2), (2, 1), (2, 2), (-3, 1)] {
            assert_eq!(cmp(a, b, s, 0), f(a, b), "{name} {a} {b}");
            assert_eq!(cmp(a, b, s, 1), f(b, a), "{name}r {a} {b}"); // reversed: left = top
        }
    }
    // s = 6: left has every bit of right (switch case 5: (a & b) == mask)
    assert_eq!(cmp(0b110, 0b100, 6, 0), 1);
    assert_eq!(cmp(0b010, 0b100, 6, 0), 0);
    assert_eq!(cmp(0b100, 0b110, 6, 1), 1);
    // an out-of-range s falls to the default case: EQ
    assert_eq!(cmp(4, 4, 9, 0), 1);
}

#[test]
fn bit_ops_and_unary() {
    // ops 0x18..0x1D
    let v = outs(&[
        push(0b1100), push(0b1010), w(op::AND, 0, 0), emit(),
        push(0b1100), push(0b1010), w(op::OR, 0, 0), emit(),
        push(0b1100), push(0b1010), w(op::XOR, 0, 0), emit(),
        push(5), w(op::NOT, 0, 0), emit(),
        push(5), w(op::NEG, 0, 0), emit(),
        push(0), w(op::LNOT, 0, 0), emit(),
        push(7), w(op::LNOT, 0, 0), emit(),
        RET,
    ]);
    assert_eq!(v, vec![0b1000, 0b1110, 0b0110, !5, -5, 1, 0]);
}

#[test]
fn shifts() {
    // op 0x1E: s = 1 arithmetic right, 2 logical right, else left; count masked to 6 bits;
    // b1 = 0: value = second word, count = top; b1 != 0: value = top, count = second
    let sh = |v: i16, c: i16, s: i16, b1: u8| outs(&[push(v), push(c), w(op::SHIFT, b1, s), emit(), RET])[0];
    assert_eq!(sh(-16, 2, 1, 0), -4);
    assert_eq!(sh(1, 4, 0, 0), 16);
    assert_eq!(sh(1, 66, 0, 0), 4); // 66 & 63 = 2
    assert_eq!(sh(2, 16, 0, 1), 64); // reversed: value = 16 (top), count = 2
    assert_eq!(sh(-1, 60, 2, 0), 15); // logical: 64-bit -1 >> 60
}

#[test]
fn locals_alloc_and_store_modes() {
    // ALLOC n lowers sp by n (op 6: sp += s * -8); frame = sp at the call, locals at frame[-1..],
    // arguments at frame[0..]; STORELOC b1 0 =, 1 +=, 2 -= (op 0x0A)
    let v = outs(&[
        w(op::ALLOC, 0, 2),
        push(10), w(op::STORE_LOCAL, 0, -1),
        push(4), w(op::STORE_LOCAL, 1, -1),
        push(3), w(op::STORE_LOCAL, 2, -1),
        w(op::PUSH_LOCAL, 0, -1), emit(),
        push(9), w(op::STORE_LOCAL, 0, -2), w(op::PUSH_LOCAL, 0, -2), emit(),
        w(op::POP, 0, 2),
        RET,
    ]);
    assert_eq!(v, vec![11, 9]);
}

#[test]
fn branches() {
    // op 7 (0x1402aab56): target = pc_after + s*4; b1 0 always, 1 if nonzero, 2 if zero (pops the value)
    let br = |b1: u8, v: i16| outs(&[push(v), w(op::BRANCH, b1, 2), push(100), emit(), push(200), emit(), RET]);
    assert_eq!(br(1, 1), vec![200]); // taken: skips `PUSH 100; emit`
    assert_eq!(br(1, 0), vec![100, 200]);
    assert_eq!(br(2, 0), vec![200]);
    assert_eq!(br(2, 1), vec![100, 200]);
    assert_eq!(outs(&[w(op::BRANCH, 0, 2), push(1), emit(), push(2), emit(), RET]), vec![2]);
    // backward branch (negative s): count down from 3
    let v = outs(&[
        w(op::ALLOC, 0, 1), push(3), w(op::STORE_LOCAL, 0, -1),
        w(op::PUSH_LOCAL, 0, -1), emit(),
        push(1), w(op::STORE_LOCAL, 2, -1),
        w(op::PUSH_LOCAL, 0, -1), w(op::BRANCH, 1, -6),
        w(op::POP, 0, 1), RET,
    ]);
    assert_eq!(v, vec![3, 2, 1]);
}

#[test]
fn yield_branch_sets_the_wait_flag() {
    // op 7 b1=3 (0x1402aab81): pop; if zero, branch AND set flag 4 (the VM returns after this op)
    let s = build_test_script(&[push(0), w(op::BRANCH, 3, 0), push(5), emit(), RET]);
    let mut vm = Vm::new(&s);
    let mut rec = Rec::default();
    vm.call_by_name("main");
    assert_eq!(vm.run(&mut rec, 100), RunStatus::Waiting);
    assert!(rec.out.is_empty());
    // the wait counter is 0, so the next run clears flag 4 and continues at the target
    assert_eq!(vm.run(&mut rec, 100), RunStatus::Finished);
    assert_eq!(rec.out, vec![5]);
    // non-zero: no branch, no yield
    let (_, r, st) = run(&[push(1), w(op::BRANCH, 3, 0), push(5), emit(), RET]);
    assert_eq!((st, r.out), (RunStatus::Finished, vec![5]));
}

#[test]
fn case_pops_on_match_only() {
    // op 8 (0x1402aabde): equal -> pop and fall through; else branch, value stays on the stack
    let v = outs(&[
        push(7),
        w(op::CASE, 3, 2), push(1), emit(), // 7 != 3: branch over `push 1; emit`
        w(op::CASE, 7, 2), push(2), emit(), // matches: pop, fall through
        RET,
    ]);
    assert_eq!(v, vec![2]);
}

#[test]
fn call_ret_arguments_values_and_frames() {
    // op 3 (0x1402aac56) pushes [pc, frame, block], frame = sp; arguments are pushed right to left so
    // a0 is the top; RET pop=N (op 5, 0x1402aac9d) drops the arguments; b1 != 0 moves the top into the
    // result register first.
    // main: [0] PUSH 4 (a1) [1] PUSH 10 (a0) [2] CALL f [3] PUSHRES [4] emit [5] RET; f at index 6
    let code = [
        push(4), push(10), w(op::CALL, 0, 3), w(op::PUSH_RESULT, 0, 0), emit(), RET,
        w(op::PUSH_LOCAL, 0, 0), w(op::PUSH_LOCAL, 0, 1), w(op::SUB, 0, 0), emit(), // a0 - a1 = 6
        w(op::PUSH_LOCAL, 0, 0), w(op::RET, 1, 2),
    ];
    let (vm, r, st) = run(&code);
    assert_eq!(st, RunStatus::Finished);
    assert_eq!(r.out, vec![6, 10]);
    assert_eq!(vm.data_stack().len(), 0, "RET pop=2 removed both arguments");
}

#[test]
fn native_arguments_order_result_and_drop() {
    // op 1 (0x1402aac34): rcx = stack pointer, so args[0] is the top (last pushed); the result goes to the
    // result register (0x1402aac3a) read by PUSHRES; b1 argument words are dropped after the call
    let s = build_test_script(&[push(3), push(2), push(1), native(4, 48, 3), w(op::PUSH_RESULT, 0, 0), emit(), RET]);
    let mut vm = Vm::new(&s);
    let mut rec = Rec { answer: 42, ..Default::default() };
    vm.call_by_name("main");
    assert_eq!(vm.run(&mut rec, 100), RunStatus::Finished);
    assert_eq!(rec.calls[0], (4, 48, vec![1, 2, 3]));
    assert_eq!(rec.out, vec![42]);
    assert_eq!(vm.data_stack().len(), 0);
    // group is bits 28..31, function bits 16..27
    assert_eq!(rec.calls[1].0, 7);
}

#[test]
fn wait_counts_down_between_runs() {
    // op 2 b1=0 (0x1402aab44): wait = s; flag 4 ends the run; each later run only decrements (0x1402aaa43)
    let s = build_test_script(&[push(1), emit(), w(op::CTRL, 0, 2), push(2), emit(), RET]);
    let mut vm = Vm::new(&s);
    let mut rec = Rec::default();
    vm.call_by_name("main");
    assert_eq!(vm.run(&mut rec, 100), RunStatus::Waiting);
    assert_eq!(rec.out, vec![1]);
    vm.run(&mut rec, 100); // wait 2 -> 1
    vm.run(&mut rec, 100); // wait 1 -> 0
    assert_eq!(rec.out, vec![1]);
    assert_eq!(vm.run(&mut rec, 100), RunStatus::Finished);
    assert_eq!(rec.out, vec![1, 2]);
}

#[test]
fn fault_unknown_opcode_and_restart() {
    // op 2 b1=1 and every unknown opcode set the fault flag (0x1402ab377)
    let (_, _, st) = run(&[w(op::CTRL, 1, 0), RET]);
    assert_eq!(st, RunStatus::Faulted);
    let (_, _, st) = run(&[0x0000_0020, RET]);
    assert_eq!(st, RunStatus::Faulted);
    let (_, _, st) = run(&[0x0000_000B, RET]);
    assert_eq!(st, RunStatus::Faulted);
    // op 2 b1=2 (0x1402aab1d): restart = idle with empty stacks; the rest of main does not run
    let (vm, r, st) = run(&[w(op::CTRL, 2, 0), push(1), emit(), RET]);
    assert_eq!(st, RunStatus::Finished);
    assert!(r.out.is_empty());
    assert!(vm.pc().is_none() && vm.is_idle());
}

#[test]
fn budget_consumption_matches_the_program() {
    // The loop head decrements the budget (0x1402aaaa9) and the handlers of branches, pushes, locals,
    // compares, CASE, ALLOC, POP and value-returning RET restore it (`mov edi, esi`, e.g. 0x1402aae63);
    // NOP, native calls, CTRL, calls, RET without value, arithmetic, bit ops and shifts consume it.
    // NOP; JMP back: budget 10 -> ten NOPs and nine branches
    let s = build_test_script(&[0, w(op::BRANCH, 0, -2)]);
    let mut vm = Vm::new(&s);
    vm.call_by_name("main");
    assert_eq!(vm.run(&mut NullNatives, 10), RunStatus::OutOfBudget);
    assert_eq!(vm.steps, 19);
    // ten pushes do not consume: they finish inside a budget of 5 ...
    let mut code: Vec<u32> = (0..10).map(|_| push(1)).collect();
    code.push(w(op::POP, 0, 10));
    code.push(w(op::RET, 0, 0));
    let s = build_test_script(&code);
    let mut vm = Vm::new(&s);
    vm.call_by_name("main");
    assert_eq!(vm.run(&mut NullNatives, 5), RunStatus::Finished);
    // ... ten NOPs do not
    let mut code: Vec<u32> = vec![0; 10];
    code.push(RET);
    let s = build_test_script(&code);
    let mut vm = Vm::new(&s);
    vm.call_by_name("main");
    assert_eq!(vm.run(&mut NullNatives, 5), RunStatus::OutOfBudget);
    // op 2 b1=3 (0x1402aab13): unlimited budget
    let mut code: Vec<u32> = vec![w(op::CTRL, 3, 0)];
    code.extend(std::iter::repeat(0).take(20));
    code.push(RET);
    let s = build_test_script(&code);
    let mut vm = Vm::new(&s);
    vm.call_by_name("main");
    assert_eq!(vm.run(&mut NullNatives, 3), RunStatus::Finished);
    // budget 0 on entry returns at once (0x1402aaa5d)
    let mut vm = Vm::new(&s);
    vm.call_by_name("main");
    assert_eq!(vm.run(&mut NullNatives, 0), RunStatus::OutOfBudget);
    assert_eq!(vm.steps, 0);
}

/// `PUSH_ADDR` encoding for an instruction at word index `at` pointing at word index `to`
/// (relative to the word after the instruction, signed 24 bits, op 0x1F at 0x1402aae95).
fn addr(at: usize, to: usize) -> u32 {
    (((to as i32) - (at as i32 + 1)) << 8) as u32 | op::PUSH_ADDR as u32
}

#[test]
fn data_pointers_and_tbl_sign_extension() {
    // g0.f16 tbl(addr, idx) (0x1402abdf0) = shadow << 32 | entry, the shadow being the sign extension
    // `entry >> 31` built by FUN_1402aa620; g0.f17 tbl_store(addr, idx, value) (0x1402abe30) writes both halves.
    let code = [
        push(2), addr(1, 12), native(0, 16, 2), w(op::PUSH_RESULT, 0, 0), emit(),
        push(0), addr(6, 12), native(0, 16, 2), w(op::PUSH_RESULT, 0, 0), emit(),
        RET, 0, // [10] RET, [11] pad
        13, (-1i32) as u32, 0xFFFF_FFF6, 77, // table at word 12: 13, -1, -10, 77
    ];
    assert_eq!(outs(&code), vec![-10, 13]);
    // store 1 << 32 | 5 at index 1 and read it back
    let code = [
        push(1), push(32), w(op::SHIFT, 0, 0), push(5), w(op::OR, 0, 0), // [0..4] the value
        push(1), addr(6, 14), native(0, 17, 3),
        push(1), addr(9, 14), native(0, 16, 2), w(op::PUSH_RESULT, 0, 0), emit(),
        RET, 0, 0, // table at word 14
    ];
    assert_eq!(outs(&code), vec![(1i64 << 32) | 5]);
    // a negative entry read back after a negative store stays -1 in 64 bits
    let code = [
        push(-1), push(0), addr(2, 11), native(0, 17, 3),
        push(0), addr(5, 11), native(0, 16, 2), w(op::PUSH_RESULT, 0, 0), emit(),
        RET, 0, 0,
    ];
    assert_eq!(outs(&code), vec![-1]);
}

#[test]
fn string_pool_addresses() {
    // op 0x11: address = pool offset + s (block + 0xC + s + block[3] with the runtime-relative header)
    let pool = b"ab\0cde\0";
    let (_, r, st) = run_with(&[w(op::PUSH_STR, 0, 0), native(7, 1, 1), w(op::PUSH_STR, 0, 3), native(7, 1, 1), RET], pool);
    assert_eq!(st, RunStatus::Finished);
    assert_eq!(r.out, vec![b'a' as i64 + b'b' as i64, b'c' as i64 + b'd' as i64 + b'e' as i64]);
}

#[test]
fn stack_misuse_faults_instead_of_crashing() {
    let (_, _, st) = run(&[w(op::ADD, 0, 0), RET]); // pop from the empty stack
    assert_eq!(st, RunStatus::Faulted);
    let (_, _, st) = run(&[w(op::RET, 0, 0), w(op::RET, 0, 0)]); // main returns, the stray RET never runs
    assert_eq!(st, RunStatus::Finished);
    // runaway recursion hits the 0x400-word stack: the program has no check, we fault
    let (_, _, st) = run(&[w(op::CALL, 0, -1)]);
    assert_eq!(st, RunStatus::Faulted);
}

#[test]
fn msvc_rand_stream_and_script_mapping() {
    // CRT rand() with srand(1): 41, 18467, 6334, 26500, 19169 (the well-known sequence)
    let mut r = MsvcRand::new(1);
    let v: Vec<u32> = (0..5).map(|_| r.next15()).collect();
    assert_eq!(v, vec![41, 18467, 6334, 26500, 19169]);
    // FUN_14020dcf0: (rand & 0x7FFF) * n >> 15 (imul eax, ebx; shr rax, 15)
    assert_eq!(script_rand(32767, 100), 99);
    assert_eq!(script_rand(0, 100), 0);
    assert_eq!(script_rand(16384, 100), 50);
}
