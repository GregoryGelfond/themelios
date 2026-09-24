//! The exhaustion gate at the public surface (docs/design/solve.md §5.3): the
//! completeness refusal is a public, `Error`-implementing value a consumer can
//! hold and chain. The gate's end-to-end behaviour over a real search — a
//! complete collection only from a closed space — is exercised through the
//! reference solver where one exists; here the refusal's public contract is
//! pinned.

use themelios_solve::outcome::NotExhausted;

#[test]
fn the_completeness_refusal_is_a_public_error_type() {
    fn assert_error<E: std::error::Error>() {}
    assert_error::<NotExhausted>();
}
