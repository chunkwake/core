//! `/return`: в ванили это выход из функции.
//!
//! Функций на сервере нет, поэтому любая форма отвечает одной понятной фразой
//! и не притворяется, что значение куда-то вернулось.

use super::dispatch::{Call, Outcome};
use serde_json::json;

const OUTSIDE: &str = "return only works inside a function, and this server does not run functions";

/// `/return <value>`, `/return fail`, `/return run …`.
pub(super) fn ret(_ctx: &super::Context<'_>, _call: &Call<'_>) -> Outcome {
    Outcome::Feedback(json!({"text": OUTSIDE, "color": "red"}))
}

pub(super) fn specs() -> Vec<super::dispatch::Spec> {
    use super::dispatch::Spec;
    use super::registry::{GAMEMASTERS, int};
    vec![
        Spec::group(
            "return",
            GAMEMASTERS,
            vec![
                Spec::literal_leaf("fail", GAMEMASTERS, ret),
                Spec::group("run", GAMEMASTERS, Vec::new()),
                Spec::arg_leaf("value", GAMEMASTERS, int(i32::MIN, i32::MAX), ret),
            ],
        )
        .with_handler(ret),
    ]
}
