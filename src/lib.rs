//! # ternary-interpreter
//!
//! Ternary bytecode interpreter for GPU control flow.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Push(i8),      // push {-1, 0, +1}
    Add,           // pop a,b -> push a+b clamped to ternary
    Mul,           // pop a,b -> push a*b
    Neg,           // pop a -> push -a
    JumpIf(usize), // pop, if +1 jump to addr
    JumpIfNeg(usize), // pop, if -1 jump to addr
    Store(usize),  // pop -> store[addr]
    Load(usize),   // store[addr] -> push
    Halt,
}

fn ternary_add(a: i8, b: i8) -> i8 {
    match (a, b) {
        (-1, -1) => -1, (-1, 0) => -1, (-1, 1) => 0,
        (0, -1) => -1, (0, 0) => 0, (0, 1) => 1,
        (1, -1) => 0, (1, 0) => 1, (1, 1) => 1,
        _ => 0,
    }
}

fn ternary_mul(a: i8, b: i8) -> i8 {
    match (a, b) {
        (-1, -1) => 1, (-1, 0) => 0, (-1, 1) => -1,
        (0, -1) => 0, (0, 0) => 0, (0, 1) => 0,
        (1, -1) => -1, (1, 0) => 0, (1, 1) => 1,
        _ => 0,
    }
}

pub struct TernaryVM {
    stack: Vec<i8>,
    store: Vec<i8>,
    pc: usize,
    halted: bool,
    steps: u64,
}

impl TernaryVM {
    pub fn new(store_size: usize) -> Self {
        Self { stack: Vec::new(), store: vec![0; store_size], pc: 0, halted: false, steps: 0 }
    }

    pub fn execute(&mut self, program: &[Op]) -> i8 {
        self.halted = false;
        self.pc = 0;
        self.stack.clear();
        while self.pc < program.len() && !self.halted {
            let op = &program[self.pc];
            self.pc += 1;
            self.steps += 1;
            match op {
                Op::Push(v) => self.stack.push(*v),
                Op::Add => {
                    let b = self.stack.pop().unwrap_or(0);
                    let a = self.stack.pop().unwrap_or(0);
                    self.stack.push(ternary_add(a, b));
                }
                Op::Mul => {
                    let b = self.stack.pop().unwrap_or(0);
                    let a = self.stack.pop().unwrap_or(0);
                    self.stack.push(ternary_mul(a, b));
                }
                Op::Neg => {
                    let a = self.stack.pop().unwrap_or(0);
                    self.stack.push(-a);
                }
                Op::JumpIf(addr) => {
                    let v = self.stack.pop().unwrap_or(0);
                    if v == 1 { self.pc = *addr; }
                }
                Op::JumpIfNeg(addr) => {
                    let v = self.stack.pop().unwrap_or(0);
                    if v == -1 { self.pc = *addr; }
                }
                Op::Store(addr) => {
                    let v = self.stack.pop().unwrap_or(0);
                    if *addr < self.store.len() { self.store[*addr] = v; }
                }
                Op::Load(addr) => {
                    if *addr < self.store.len() { self.stack.push(self.store[*addr]); }
                }
                Op::Halt => { self.halted = true; }
            }
        }
        self.stack.pop().unwrap_or(0)
    }

    pub fn stack_top(&self) -> Option<i8> { self.stack.last().copied() }
    pub fn stack_len(&self) -> usize { self.stack.len() }
    pub fn store(&self, addr: usize) -> i8 { self.store.get(addr).copied().unwrap_or(0) }
    pub fn steps(&self) -> u64 { self.steps }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_halt() {
        let mut vm = TernaryVM::new(16);
        let result = vm.execute(&[Op::Push(1), Op::Halt]);
        assert_eq!(result, 1);
    }

    #[test]
    fn test_add() {
        let mut vm = TernaryVM::new(16);
        let result = vm.execute(&[Op::Push(1), Op::Push(-1), Op::Add, Op::Halt]);
        assert_eq!(result, 0); // 1 + (-1) = 0
    }

    #[test]
    fn test_mul() {
        let mut vm = TernaryVM::new(16);
        let result = vm.execute(&[Op::Push(-1), Op::Push(-1), Op::Mul, Op::Halt]);
        assert_eq!(result, 1); // (-1) * (-1) = +1
    }

    #[test]
    fn test_neg() {
        let mut vm = TernaryVM::new(16);
        let result = vm.execute(&[Op::Push(1), Op::Neg, Op::Halt]);
        assert_eq!(result, -1);
    }

    #[test]
    fn test_store_load() {
        let mut vm = TernaryVM::new(16);
        vm.execute(&[Op::Push(1), Op::Store(5), Op::Load(5), Op::Halt]);
        assert_eq!(vm.store(5), 1);
    }

    #[test]
    fn test_conditional_jump() {
        let mut vm = TernaryVM::new(16);
        // Push 1, jump over Push(-1)
        let result = vm.execute(&[
            Op::Push(1),       // 0
            Op::JumpIf(4),     // 1: if +1 jump to 4
            Op::Push(-1),      // 2: skipped
            Op::Halt,          // 3: skipped
            Op::Push(1),       // 4
            Op::Halt,          // 5
        ]);
        assert_eq!(result, 1);
    }

    #[test]
    fn test_jump_if_neg() {
        let mut vm = TernaryVM::new(16);
        let result = vm.execute(&[
            Op::Push(-1),
            Op::JumpIfNeg(3),  // jump to Halt that returns 1
            Op::Push(0),
            Op::Push(1),
            Op::Halt,
        ]);
        assert_eq!(result, 1);
    }

    #[test]
    fn test_step_count() {
        let mut vm = TernaryVM::new(16);
        vm.execute(&[Op::Push(1), Op::Push(1), Op::Add, Op::Halt]);
        assert_eq!(vm.steps(), 4);
    }

    #[test]
    fn test_complex_program() {
        let mut vm = TernaryVM::new(16);
        // Compute: 1 * 1 + (-1) = 0
        let result = vm.execute(&[
            Op::Push(1), Op::Push(1), Op::Mul,  // 1*1 = 1
            Op::Push(-1),                         // push -1
            Op::Add,                              // 1 + (-1) = 0
            Op::Halt,
        ]);
        assert_eq!(result, 0);
    }
}
