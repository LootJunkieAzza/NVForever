//! xNVSE's token functions (docs/handoff/xnvse-remaining.md §3, "tokens"): a number and
//! a form kept on a token item in an actor's inventory [documented: GetTokenValue;
//! GetTokenRef; SetTokenValue; SetTokenRef; SetTokenValueAndRef]. The wiki's page on what
//! makes an item a token (Category:Tokens) is not in the export, and the base game has
//! no item that takes a token value: on a carried stimpak every setter is ignored and
//! both getters read 0 [observed: nvse-sixth, token.* rows]. So no item is a token here
//! until that page or a probe on a real token says how one is marked (owner-review).

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;

/// The functions of this file; `None` for any other name.
pub(crate) fn call(_g: &mut Game, name: &str, _subject: Option<FormId>, _args: &[XVal]) -> Option<Result<XVal, String>> {
    let v = match name {
        "gettokenvalue" => XVal::Num(0.0),
        "gettokenref" => XVal::Ref(0),
        "settokenvalue" | "settokenref" | "settokenvalueandref" => XVal::Num(0.0),
        _ => return None,
    };
    Some(Ok(v))
}
