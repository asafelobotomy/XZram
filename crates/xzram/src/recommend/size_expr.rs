//! Evaluate zram-generator size expressions (`min(ram / 2, 4096)`, `ram * 1.5`, ...) to MiB.
//!
//! zram-generator evaluates `zram-size` with a small arithmetic language where `ram` is the
//! installed memory in MiB. This covers the subset XZram writes and users commonly use:
//! numbers (integer or decimal), `ram`, `+ - * /`, parentheses and `min()` / `max()`.

/// Evaluate `formula` for `ram_mb` of installed memory. `None` when the expression is not
/// understood or evaluates to a negative / non-finite value.
pub fn eval_zram_size_mb(formula: &str, ram_mb: u64) -> Option<u64> {
    let tokens = tokenize(formula)?;
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        ram: ram_mb as f64,
        depth: 0,
    };
    let value = parser.expr()?;
    if parser.pos != tokens.len() || !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(value.floor() as u64)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Ident(String),
    Op(char),
}

fn tokenize(input: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Token::Num(text.parse().ok()?));
        } else if c.is_ascii_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Token::Ident(text.to_ascii_lowercase()));
        } else if "+-*/(),".contains(c) {
            out.push(Token::Op(c));
            i += 1;
        } else {
            return None;
        }
    }
    Some(out)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    ram: f64,
    /// Nesting so far: parentheses, `min`/`max` and unary minus recurse.
    depth: u32,
}

/// Deepest nesting accepted, so a pathological formula can't exhaust the stack.
const MAX_DEPTH: u32 = 32;

impl Parser<'_> {
    fn peek_op(&self) -> Option<char> {
        match self.tokens.get(self.pos) {
            Some(Token::Op(c)) => Some(*c),
            _ => None,
        }
    }

    fn eat(&mut self, op: char) -> Option<()> {
        (self.peek_op() == Some(op)).then(|| self.pos += 1)
    }

    fn expr(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        while let Some(op @ ('+' | '-')) = self.peek_op() {
            self.pos += 1;
            let rhs = self.term()?;
            value = if op == '+' { value + rhs } else { value - rhs };
        }
        Some(value)
    }

    fn term(&mut self) -> Option<f64> {
        let mut value = self.factor()?;
        while let Some(op @ ('*' | '/')) = self.peek_op() {
            self.pos += 1;
            let rhs = self.factor()?;
            value = if op == '*' { value * rhs } else { value / rhs };
        }
        Some(value)
    }

    fn factor(&mut self) -> Option<f64> {
        if self.depth >= MAX_DEPTH {
            return None;
        }
        self.depth += 1;
        let value = self.factor_inner();
        self.depth -= 1;
        value
    }

    fn factor_inner(&mut self) -> Option<f64> {
        let token = self.tokens.get(self.pos)?.clone();
        self.pos += 1;
        match token {
            Token::Num(n) => Some(n),
            Token::Op('(') => {
                let v = self.expr()?;
                self.eat(')')?;
                Some(v)
            }
            Token::Op('-') => Some(-self.factor()?),
            Token::Ident(name) if name == "ram" => Some(self.ram),
            Token::Ident(name) if name == "min" || name == "max" => {
                self.eat('(')?;
                let a = self.expr()?;
                self.eat(',')?;
                let b = self.expr()?;
                self.eat(')')?;
                Some(if name == "min" { a.min(b) } else { a.max(b) })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_common_formulas() {
        let ram = 15_872; // 15.5 GiB
        assert_eq!(eval_zram_size_mb("ram", ram), Some(ram));
        assert_eq!(eval_zram_size_mb("min(ram / 2, 4096)", ram), Some(4096));
        assert_eq!(eval_zram_size_mb("min(ram, 8192)", ram), Some(8192));
        assert_eq!(eval_zram_size_mb("min(ram, 16384)", ram), Some(ram));
        assert_eq!(eval_zram_size_mb("ram * 1.5", ram), Some(23_808));
        assert_eq!(
            eval_zram_size_mb("min(ram * 1.5, 24576)", ram),
            Some(23_808)
        );
        assert_eq!(eval_zram_size_mb("ram / 100 * 50", 8192), Some(4096));
        assert_eq!(eval_zram_size_mb("max(ram / 4, 1024)", 2048), Some(1024));
        assert_eq!(eval_zram_size_mb("512", 8192), Some(512));
        assert_eq!(eval_zram_size_mb("(ram + 1024) / 2", 1024), Some(1024));
    }

    #[test]
    fn rejects_unknown_or_malformed() {
        assert_eq!(eval_zram_size_mb("swap * 2", 8192), None);
        assert_eq!(eval_zram_size_mb("min(ram", 8192), None);
        assert_eq!(eval_zram_size_mb("ram ram", 8192), None);
        assert_eq!(eval_zram_size_mb("-ram", 8192), None);
        assert_eq!(eval_zram_size_mb("", 8192), None);
    }

    #[test]
    fn refuses_pathological_nesting() {
        let deep = format!("{}ram{}", "(".repeat(100_000), ")".repeat(100_000));
        assert_eq!(eval_zram_size_mb(&deep, 8192), None);
        let ok = format!("{}ram{}", "(".repeat(10), ")".repeat(10));
        assert_eq!(eval_zram_size_mb(&ok, 8192), Some(8192));
    }
}
