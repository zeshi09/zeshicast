pub(crate) fn looks_like_expression(value: &str) -> bool {
    let has_operator = value.chars().any(|ch| matches!(ch, '+' | '-' | '*' | '/'));
    let only_expr_chars = value.chars().all(|ch| {
        ch.is_ascii_digit()
            || ch.is_ascii_whitespace()
            || matches!(ch, '.' | '+' | '-' | '*' | '/' | '(' | ')')
    });
    has_operator && only_expr_chars
}

pub(crate) fn format_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.8}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

pub(crate) struct Calculator<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Calculator<'a> {
    pub(crate) fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    pub(crate) fn parse(mut self) -> Result<f64, String> {
        let value = self.expression()?;
        self.skip_ws();
        if self.pos == self.input.len() {
            Ok(value)
        } else {
            Err("unexpected trailing characters".to_string())
        }
    }

    fn expression(&mut self) -> Result<f64, String> {
        let mut value = self.term()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'+') => {
                    self.pos += 1;
                    value += self.term()?;
                }
                Some(b'-') => {
                    self.pos += 1;
                    value -= self.term()?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn term(&mut self) -> Result<f64, String> {
        let mut value = self.factor()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'*') => {
                    self.pos += 1;
                    value *= self.factor()?;
                }
                Some(b'/') => {
                    self.pos += 1;
                    let divisor = self.factor()?;
                    if divisor == 0.0 {
                        return Err("division by zero".to_string());
                    }
                    value /= divisor;
                }
                _ => return Ok(value),
            }
        }
    }

    fn factor(&mut self) -> Result<f64, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'-') => {
                self.pos += 1;
                Ok(-self.factor()?)
            }
            Some(b'(') => {
                self.pos += 1;
                let value = self.expression()?;
                self.skip_ws();
                if self.peek() != Some(b')') {
                    return Err("missing ')'".to_string());
                }
                self.pos += 1;
                Ok(value)
            }
            _ => self.number(),
        }
    }

    fn number(&mut self) -> Result<f64, String> {
        self.skip_ws();
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9') | Some(b'.')) {
            self.pos += 1;
        }
        if start == self.pos {
            return Err("expected number".to_string());
        }
        std::str::from_utf8(&self.input[start..self.pos])
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .ok_or_else(|| "invalid number".to_string())
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }
}
