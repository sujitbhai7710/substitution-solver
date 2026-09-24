//! Debug: name_bonus and bigram scores on truth vs got texts.
use subst_solver::name_bonus;
fn main() {
    let cases: Vec<(&str, &str)> = vec![
        ("liv-truth", "\u{201c}THERE\u{2019}S NO CREAM THAT CAN FIX YOU IF YOU\u{2019}RE NOT BEAUTIFUL ON THE INSIDE.\u{201d} \u{2013} LIV TYLER"),
        ("liv-got", "\u{201c}THERE\u{2019}S NO CREAM THAT CAN FIX YOU IF YOU\u{2019}RE NOT BEAUTIFUL ON THE INSIDE.\u{201d} \u{2013} LIP TYLER"),
        ("james-truth", "LOVE TAKES OFF THE MASKS. \u{2013} JAMES BALDWIN"),
        ("james-got", "LOVE TAKES OFF THE MASKS. \u{2013} GAMES BALDWIN"),
        ("serena-truth", "I REALLY THINK A CHAMPION. \u{2013} SERENA WILLIAMS"),
    ];
    for (name, t) in cases {
        println!("{}: name_bonus={}", name, name_bonus(t.as_bytes()));
    }
}
