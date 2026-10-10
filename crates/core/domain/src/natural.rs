use std::cmp::Ordering;

use crate::path::fold_char;

#[derive(Clone, Copy)]
enum Token<'a> {
    Digits(&'a str),
    Char(char),
}

pub fn compare(left: &str, right: &str) -> Ordering {
    let mut left_tokens = tokens(left);
    let mut right_tokens = tokens(right);
    loop {
        let order = match (left_tokens.next(), right_tokens.next()) {
            (None, None) => return left.cmp(right),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) => compare_tokens(a, b),
        };
        if order.is_ne() {
            return order;
        }
    }
}

pub fn sort_key(text: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(text.len() * 5 + 1);
    for token in tokens(text) {
        key.push(1);
        match token {
            Token::Digits(run) => {
                let number = run.trim_start_matches('0');
                key.extend_from_slice(&u32::from('0').to_be_bytes());
                key.extend_from_slice(
                    &u32::try_from(number.len())
                        .unwrap_or(u32::MAX)
                        .to_be_bytes(),
                );
                key.extend_from_slice(number.as_bytes());
            }
            Token::Char(c) => key.extend_from_slice(&u32::from(fold_char(c)).to_be_bytes()),
        }
    }
    key.push(0);
    key.extend_from_slice(text.as_bytes());
    key
}

fn compare_tokens(left: Token<'_>, right: Token<'_>) -> Ordering {
    match (left, right) {
        (Token::Digits(a), Token::Digits(b)) => compare_numbers(a, b),
        (Token::Digits(a), Token::Char(c)) => a.chars().next().cmp(&Some(fold_char(c))),
        (Token::Char(c), Token::Digits(b)) => Some(fold_char(c)).cmp(&b.chars().next()),
        (Token::Char(a), Token::Char(b)) => fold_char(a).cmp(&fold_char(b)),
    }
}

fn compare_numbers(left: &str, right: &str) -> Ordering {
    let left = left.trim_start_matches('0');
    let right = right.trim_start_matches('0');
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn tokens(text: &str) -> impl Iterator<Item = Token<'_>> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0 {
            let (run, tail) = rest.split_at(digits);
            rest = tail;
            return Some(Token::Digits(run));
        }
        let mut chars = rest.chars();
        let next = chars.next()?;
        rest = chars.as_str();
        Some(Token::Char(next))
    })
}
