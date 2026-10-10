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
