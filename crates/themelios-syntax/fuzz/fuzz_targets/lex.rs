//! Arbitrary bytes under both dialects and all three modes: admission
//! refuses what is not UTF-8; every admitted text tiles under every mode
//! from offset zero, no token is empty, every token is a slice, and the
//! four token-source laws hold on the file lexer
//! (docs/design/syntax.md §4.3, §16).
#![no_main]

use libfuzzer_sys::fuzz_target;
use themelios_base::source::{Source, SourceId};
use themelios_base::span::ByteOffset;
use themelios_syntax::dialect::Dialect;
use themelios_syntax::lexer::Lexer;
use themelios_syntax::token::{LexMode, TokenSource, check_token_source_laws};
use themelios_syntax::tree::SyntaxKind;

fuzz_target!(|data: &[u8]| {
    let Ok(source) = Source::from_bytes(SourceId::new(0), data.to_vec()) else {
        return;
    };
    for dialect in [Dialect::Clingo, Dialect::AspCore2] {
        let lexer = Lexer::new(&source, dialect);
        assert!(check_token_source_laws(&lexer).is_empty());
        for mode in [LexMode::Normal, LexMode::Theory, LexMode::ScriptBody] {
            let text = source.text();
            let mut at = 0usize;
            loop {
                let token = lexer
                    .token_at(ByteOffset::new(u32::try_from(at).expect("admitted")), mode)
                    .expect("a position");
                if token.kind == SyntaxKind::EOF {
                    assert_eq!(at, text.len());
                    break;
                }
                assert!(!token.text.is_empty());
                assert_eq!(&text[at..at + token.text.len()], token.text);
                at += token.text.len();
            }
        }
    }
});
