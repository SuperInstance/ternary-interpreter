# ternary-interpreter

**A stack-based bytecode VM where every value is {-1, 0, +1}. Built for GPU control flow, useful for thinking about ternary computation.**

## Why This Exists

Most bytecode VMs operate on integers or floats — rich number systems with无穷 possible values. This one operates on exactly three: negative, zero, positive. That constraint sounds limiting. It's actually clarifying.

When you restrict a VM to ternary values, something interesting happens: every operation has a crisp semantic meaning. `Add` isn't "arbitrary integer addition" — it's Z₃ group addition, which wraps around. `Mul` isn't "arbitrary multiplication" — it's sign multiplication, the simplest non-trivial binary operation. `JumpIf` doesn't branch on "truthy vs falsy" — it branches on *positive* specifically, with `JumpIfNeg` handling the negative case. Zero is genuinely neutral: it doesn't trigger either branch.

This maps directly to how GPU warp voting works. A warp ballot returns three meaningful states per lane: yes, no, abstain. Control flow decisions on GPUs are already ternary — this VM just makes it explicit.

## The Key Insight

Traditional VMs encode decisions as `if (value != 0)`. Ternary VMs encode decisions as *which* non-zero value. The difference matters:

```
Binary branching:   value != 0 → true    (2 outcomes, 1 meaningful)
Ternary branching:  value > 0  → yes     (3 outcomes, 3 meaningful)
                    value < 0  → no
                    value == 0 → neither
```

This three-way branching is why `JumpIf` and `JumpIfNeg` are separate instructions. Zero doesn't jump. It's the "keep going" signal. For consensus protocols — which is what GPU warp voting actually is — this is exactly what you need.

## Quick Start

```rust
use ternary_interpreter::{TernaryVM, Op};

// Simple computation: 1 × 1 + (-1) = 0
let mut vm = TernaryVM::new(16); // 16-cell store

let result = vm.execute(&[
    Op::Push(1),     // stack: [1]
    Op::Push(1),     // stack: [1, 1]
    Op::Mul,         // stack: [1]   (1 × 1 = 1)
    Op::Push(-1),    // stack: [1, -1]
    Op::Add,         // stack: [0]   (1 + (-1) = 0 in Z₃)
    Op::Halt,
]);
assert_eq!(result, 0);

// Conditional branching: skip code based on sign
let result = vm.execute(&[
    Op::Push(1),       // 0: push positive value
    Op::JumpIf(4),     // 1: if +1, jump to address 4
    Op::Push(-1),      // 2: skipped
    Op::Halt,          // 3: skipped
    Op::Push(1),       // 4: landed here
    Op::Halt,          // 5
]);
assert_eq!(result, 1);
```

## Architecture

### The VM

```
┌─────────────────────────────────┐
│          TernaryVM              │
│  ┌───────┐  ┌───────────────┐   │
│  │ Stack │  │  Store[0..N]  │   │
│  │ Vec<i8>│  │  Vec<i8>     │   │
│  └───────┘  └───────────────┘   │
│  PC: usize   Steps: u64         │
│  Halted: bool                   │
└─────────────────────────────────┘
```

- **Stack**: Unbounded, holds ternary values {-1, 0, +1}. Operations pop operands and push results.
- **Store**: Fixed-size random-access memory (specified at construction). Initialized to all zeros.
- **PC**: Program counter — index into the instruction array.
- **Steps**: Total instructions executed (useful for gas accounting).

### Instruction Set

| Instruction | Stack Effect | Description |
|-------------|-------------|-------------|
| `Push(v)` | → v | Push a ternary value {-1, 0, +1} |
| `Add` | a, b → a+b | Z₃ addition (wraps: 1+1=1, not 2) |
| `Mul` | a, b → a×b | Sign multiplication |
| `Neg` | a → -a | Negate: +1↔-1, 0→0 |
| `JumpIf(addr)` | v → | If v == +1, set PC to addr |
| `JumpIfNeg(addr)` | v → | If v == -1, set PC to addr |
| `Store(addr)` | v → | Pop to store[addr] |
| `Load(addr)` | → store[addr] | Push from store[addr] |
| `Halt` | — | Stop execution, return TOS |

### Ternary Arithmetic

The `Add` operation uses Z₃ (mod 3) addition, not integer addition. The full truth table:

```
Add:           Mul:           Neg:
+1 +1 → +1    +1 ×+1 → +1    -(-1) → +1
+1  0 → +1    +1 × 0 →  0    -( 0) →  0
+1 -1 →  0    +1 ×-1 → -1    -(+1) → -1
 0 +1 → +1     0 ×+1 →  0
 0  0 →  0     0 × 0 →  0
 0 -1 → -1     0 ×-1 →  0
-1 +1 →  0    -1 ×+1 → -1
-1  0 → -1    -1 × 0 →  0
-1 -1 → -1    -1 ×-1 → +1
```

Notice: `1 + 1 = 1`, not 2. Z₃ wraps around. And `(-1) + (-1) = -1`. This is group addition, not integer addition.

## API Reference

### TernaryVM

| Method | Signature | Description |
|--------|-----------|-------------|
| `new` | `(store_size: usize) → TernaryVM` | Create VM with N-cell store |
| `execute` | `(&mut self, program: &[Op]) → i8` | Run program, return top-of-stack |
| `stack_top` | `(&self) → Option<i8>` | Peek at top of stack |
| `stack_len` | `(&self) → usize` | Current stack depth |
| `store` | `(&self, addr: usize) → i8` | Read a store cell |
| `steps` | `(&self) → u64` | Total instructions executed |

### Op

```rust
pub enum Op {
    Push(i8),          // Push ternary value
    Add,               // Z₃ addition
    Mul,               // Sign multiplication
    Neg,               // Negate
    JumpIf(usize),     // Jump if positive
    JumpIfNeg(usize),  // Jump if negative
    Store(usize),      // Pop → store
    Load(usize),       // Store → push
    Halt,              // Stop execution
}
```

## Real-World Example: Ternary Consensus Protocol

```rust
use ternary_interpreter::{TernaryVM, Op};

// Simulate a 3-node consensus vote
// Node 0: +1 (yes), Node 1: 0 (abstain), Node 2: +1 (yes)
// Decision: sum all votes, if positive → accept, if negative → reject, if zero → pending

let mut vm = TernaryVM::new(8);

// Store votes
vm.execute(&[Op::Push(1),  Op::Store(0)]);  // Node 0: yes
vm.execute(&[Op::Push(0),  Op::Store(1)]);  // Node 1: abstain
vm.execute(&[Op::Push(1),  Op::Store(2)]);  // Node 2: yes

// Compute consensus
let result = vm.execute(&[
    Op::Load(0),     // Push node 0's vote
    Op::Load(1),     // Push node 1's vote
    Op::Add,         // Partial sum
    Op::Load(2),     // Push node 2's vote
    Op::Add,         // Full sum: 1 + 0 + 1 = 1 in Z₃
    Op::Halt,
]);
// result == 1 → consensus reached (positive)
```

## Design Decisions

**Why stack-based?** Register-based VMs are faster for real workloads, but stack machines are simpler to reason about and compile to. For a VM whose primary purpose is modeling ternary control flow, simplicity wins.

**Why `i8` for trits?** Rust doesn't have a native ternary type. `i8` is the smallest signed integer, and {-1, 0, +1} fits naturally. On GPUs, these would be packed 16-per-u32 (2 bits each), but the VM operates on unpacked values for clarity.

**Why separate `JumpIf` and `JumpIfNeg`?** A single `Branch(op, addr)` would be more compact, but ternary branching is fundamentally three-valued. Having two jump instructions makes the three-way nature explicit: positive jumps here, negative jumps there, zero falls through.

**Why `steps()` counter?** For gas-style execution metering. In a GPU context, you want to know exactly how many instructions a kernel consumed. The step counter gives you this without any external instrumentation.

## Ecosystem Connections

- **`ternary-fuse`** — Fuses VM-like operation chains into single-pass kernels
- **`ternary-dispatch`** — Queues and dispatches ternary-packed GPU kernels
- **`ternary-compiler`** — Higher-level expression compiler that emits `Op` sequences
- **`ternary-wasm`** — Browser-based ternary engine (different VM, same algebra)

## Open Questions

- **Packing**: Should the VM support packed trit operations (16 trits per u32)? It would be faster but change the programming model.
- **Subroutines**: No `Call`/`Return` instructions yet. For complex control flow, you'd need them.
- **Parallel execution**: Multiple VMs running the same program with different store states — useful for warp-level simulation.
- **Formal verification**: The Z₃ arithmetic is simple enough that the whole VM could be formally verified with something like Kani or Prusti.

## Stats

| Metric | Value |
|--------|-------|
| Lines of Rust | ~187 |
| Tests | 9 |
| Instructions | 9 |
| Dependencies | 0 |

## License

Apache-2.0
