//! M3.x — chunking for prompts exceeding the context limit of a single node.

pub const CHAR_PER_TOKEN_WORST_CASE: f64 = 1.2;
pub const ZNAK_NA_TOKEN_WORST_CASE: f64 = CHAR_PER_TOKEN_WORST_CASE;

pub const CHAR_PER_TOKEN_GENERATED_PROSE: f64 = 3.5;
pub const ZNAK_NA_TOKEN_PROZA_GENEROWANA: f64 = CHAR_PER_TOKEN_GENERATED_PROSE;

/// Splits `text` into chunks of character length <= `char_budget`.
/// Slicing respects `char` boundaries for safe UTF-8 multibyte characters.
pub fn split_into_chunks(text: &str, char_budget: usize) -> Vec<String> {
    if text.is_empty() || char_budget == 0 {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(char_budget)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

pub fn podziel_na_kawalki(tekst: &str, budzet_znakow: usize) -> Vec<String> {
    split_into_chunks(tekst, budzet_znakow)
}

/// Estimates token count without invoking a full tokenizer.
#[allow(dead_code)]
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.chars()
        .count()
        .div_ceil(4)
        .max(text.split_whitespace().count())
}

#[allow(dead_code)]
pub fn oszacuj_tokeny(tekst: &str) -> usize {
    estimate_tokens(tekst)
}

/// Packs `items` into sequential groups by token budget.
#[allow(dead_code)]
pub fn group_by_token_budget(items: &[String], token_budget: usize) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut current_tokens = 0usize;

    for item in items {
        let tok = estimate_tokens(item);
        if !current.is_empty() && current_tokens + tok > token_budget {
            groups.push(std::mem::take(&mut current));
            current_tokens = 0;
        }
        current_tokens += tok;
        current.push(item.clone());
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

#[allow(dead_code)]
pub fn grupuj_do_budzetu_tokenow(elementy: &[String], budzet_tokenow: usize) -> Vec<Vec<String>> {
    group_by_token_budget(elementy, budzet_tokenow)
}

/// Packs `items` into sequential groups whose total character length fits `char_budget`.
pub fn group_by_char_budget(items: &[String], char_budget: usize) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut current_len = 0usize;

    for item in items {
        let len = item.chars().count();
        if !current.is_empty() && current_len + len > char_budget {
            groups.push(std::mem::take(&mut current));
            current_len = 0;
        }
        current_len += len;
        current.push(item.clone());
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

pub fn grupuj_do_budzetu(elementy: &[String], budzet_znakow: usize) -> Vec<Vec<String>> {
    group_by_char_budget(elementy, budzet_znakow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_yields_empty_result() {
        assert!(split_into_chunks("", 100).is_empty());
    }

    #[test]
    fn zero_budget_yields_empty_result() {
        assert!(split_into_chunks("anything", 0).is_empty());
    }

    #[test]
    fn text_fits_in_single_chunk() {
        let chunks = split_into_chunks("short text", 1000);
        assert_eq!(chunks, vec!["short text".to_string()]);
    }

    #[test]
    fn text_splits_into_n_chunks() {
        let text = "a".repeat(250);
        let chunks = split_into_chunks(&text, 100);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[1].len(), 100);
        assert_eq!(chunks[2].len(), 50);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn chunking_safe_on_multibyte_utf8_boundaries() {
        let text = "\u{0105}\u{0107}\u{0119}\u{0142}\u{0144}\u{00f3}\u{015b}\u{017a}\u{017c}".repeat(5);
        let chunks = split_into_chunks(&text, 3);
        assert_eq!(chunks.concat(), text, "roundtrip must be lossless");
        for k in &chunks {
            assert!(k.chars().count() <= 3);
        }
    }

    #[test]
    fn grouping_packs_within_budget() {
        let items = vec!["aaaa".to_string(), "bbbb".to_string(), "cc".to_string()];
        let groups = group_by_char_budget(&items, 8);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec!["aaaa".to_string(), "bbbb".to_string()]);
        assert_eq!(groups[1], vec!["cc".to_string()]);
    }

    #[test]
    fn grouping_preserves_order() {
        let items: Vec<String> = (0..10).map(|i| format!("el{i}")).collect();
        let groups = group_by_char_budget(&items, 6);
        let flattened: Vec<String> = groups.into_iter().flatten().collect();
        assert_eq!(flattened, items);
    }

    #[test]
    fn oversized_item_gets_own_group() {
        let items = vec!["x".repeat(100)];
        let groups = group_by_char_budget(&items, 10);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 1);
    }

    #[test]
    fn many_items_reduce_to_multiple_groups() {
        let items: Vec<String> = (0..72).map(|_| "x".repeat(300)).collect();
        let groups = group_by_char_budget(&items, 4000);
        assert!(groups.len() > 1);
        assert!(groups.len() < items.len());
    }

    #[test]
    fn test_estimate_tokens_empty_and_simple() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("test"), 1);
        assert_eq!(estimate_tokens("a b c d e"), 5);
        assert_eq!(estimate_tokens("superkalifragilisti"), 5);
    }

    #[test]
    fn test_estimate_tokens_multibyte_utf8() {
        let text = "\u{017b}a\u{017c}\u{00f3}\u{0142}\u{0107} g\u{0119}\u{015b}l\u{0105} ja\u{017a}\u{0144}";
        assert_eq!(estimate_tokens(text), 5);
    }

    #[test]
    fn test_split_into_chunks_multibyte_and_emojis() {
        let text = "\u{1f680}za\u{017c}\u{00f3}\u{0142}\u{0107}\u{1f31f}g\u{0119}\u{015b}l\u{0105}\u{2728}ja\u{017a}\u{0144}";
        let chunks = split_into_chunks(text, 3);
        assert_eq!(chunks.concat(), text);
        for k in &chunks {
            assert!(k.chars().count() <= 3);
        }
    }

    #[test]
    fn test_group_by_char_budget_order_preservation() {
        let items: Vec<String> = (0..25).map(|i| format!("batch_{i:04}")).collect();
        let groups = group_by_char_budget(&items, 35);
        assert!(groups.len() > 1);
        let flattened: Vec<String> = groups.into_iter().flatten().collect();
        assert_eq!(flattened, items);
    }

    #[test]
    fn test_group_by_token_budget_order_preservation() {
        let items: Vec<String> = (0..20)
            .map(|i| format!("chunk number {i} text"))
            .collect();
        let groups = group_by_token_budget(&items, 10);
        assert!(groups.len() > 1);
        let flattened: Vec<String> = groups.into_iter().flatten().collect();
        assert_eq!(flattened, items);
    }
}
