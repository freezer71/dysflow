//! Mise en mots des nombres en français.
//!
//! Un moteur TTS lit « 2029 » comme une suite de chiffres — « deux zéro deux
//! neuf » — ou hésite. Écrire le nombre en toutes lettres retire la décision
//! au moteur.

/// Chiffres et irréguliers jusqu'à dix-neuf : au-delà, tout se compose.
const UNITS: [&str; 20] = [
    "zéro", "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf", "dix",
    "onze", "douze", "treize", "quatorze", "quinze", "seize", "dix-sept", "dix-huit", "dix-neuf",
];

/// Dizaines régulières. Le français compte par vingt au-delà de soixante :
/// 70 et 90 sont traités à part, pas par cette table.
const REGULAR_TENS: [&str; 6] = ["vingt", "trente", "quarante", "cinquante", "soixante", ""];

/// Multiplicateurs, du plus grand au plus petit.
///
/// « mille » est invariable et implicite (jamais « un mille ») ; « million »
/// et « milliard » sont des noms ordinaires, donc accordés.
const SCALES: &[(u64, &str)] = &[(1_000_000_000, "milliard"), (1_000_000, "million"), (1_000, "mille")];

/// Écrit `n` en toutes lettres, orthographe française classique.
pub fn spell_cardinal(n: u64) -> String {
    spell_scaled(n, true)
}

/// `standalone` indique que rien ne suit le nombre écrit.
///
/// Le pluriel de « quatre-vingts » et de « cents » tombe dès qu'un mot les
/// suit : « quatre-vingts » mais « quatre-vingt mille », « deux cents » mais
/// « deux cent un ».
fn spell_scaled(n: u64, standalone: bool) -> String {
    if n < 1000 {
        return spell_below_thousand(n, standalone);
    }
    for &(value, name) in SCALES {
        if n < value {
            continue;
        }
        let count = n / value;
        let rest = n % value;
        let mut out = String::new();
        if name == "mille" {
            if count > 1 {
                out.push_str(&spell_below_thousand(count, false));
                out.push(' ');
            }
            out.push_str("mille");
        } else {
            out.push_str(&spell_scaled(count, false));
            out.push(' ');
            out.push_str(name);
            if count > 1 {
                out.push('s');
            }
        }
        if rest > 0 {
            out.push(' ');
            out.push_str(&spell_scaled(rest, standalone));
        }
        return out;
    }
    unreachable!("n >= 1000 relève forcément d'une échelle")
}

fn spell_below_thousand(n: u64, standalone: bool) -> String {
    let hundreds = n / 100;
    let rest = n % 100;
    if hundreds == 0 {
        return spell_below_hundred(rest, standalone);
    }
    // « cent » ne se fait pas précéder de « un ».
    let mut out = if hundreds == 1 {
        "cent".to_string()
    } else {
        format!("{} cent", UNITS[hundreds as usize])
    };
    if rest == 0 {
        if hundreds > 1 && standalone {
            out.push('s');
        }
    } else {
        out.push(' ');
        out.push_str(&spell_below_hundred(rest, standalone));
    }
    out
}

fn spell_below_hundred(n: u64, standalone: bool) -> String {
    if n < 20 {
        return UNITS[n as usize].to_string();
    }
    match n / 10 {
        // Soixante et quatre-vingt portent chacun une plage de vingt : c'est
        // « soixante-dix-neuf », pas « septante-neuf ».
        6 | 7 => match n - 60 {
            0 => "soixante".to_string(),
            1 => "soixante et un".to_string(),
            11 => "soixante et onze".to_string(),
            rest => format!("soixante-{}", UNITS[rest as usize]),
        },
        8 | 9 => match n - 80 {
            0 if standalone => "quatre-vingts".to_string(),
            0 => "quatre-vingt".to_string(),
            rest => format!("quatre-vingt-{}", UNITS[rest as usize]),
        },
        tens => {
            let name = REGULAR_TENS[(tens - 2) as usize];
            match n % 10 {
                0 => name.to_string(),
                // « et » soude la seule unité, sans traits d'union.
                1 => format!("{name} et un"),
                unit => format!("{name}-{}", UNITS[unit as usize]),
            }
        }
    }
}

/// Écrit l'ordinal de `n` : « quatrième », « vingt et unième ».
///
/// `feminine` ne change que le premier rang — c'est le seul ordinal français
/// dont la forme varie.
pub fn spell_ordinal(n: u64, feminine: bool) -> String {
    if n == 1 {
        return if feminine { "première" } else { "premier" }.to_string();
    }
    let mut base = spell_cardinal(n);
    // Le pluriel de « quatre-vingts » et « cents » tombe devant le suffixe —
    // mais le s de « trois » ou « six » n'est pas une marque de pluriel.
    if base.ends_with("vingts") || base.ends_with("cents") {
        base.pop();
    }
    if base.ends_with('e') {
        // quatre → quatrième, onze → onzième, mille → millième
        base.pop();
    } else if base.ends_with('q') {
        // cinq → cinquième
        base.push('u');
    } else if base.ends_with('f') {
        // neuf → neuvième
        base.pop();
        base.push('v');
    }
    base.push_str("ième");
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_the_first_rank_apart() {
        assert_eq!(spell_ordinal(1, false), "premier");
        assert_eq!(spell_ordinal(1, true), "première");
    }

    #[test]
    fn builds_other_ordinals_on_the_cardinal() {
        assert_eq!(spell_ordinal(2, false), "deuxième");
        assert_eq!(spell_ordinal(3, false), "troisième");
        assert_eq!(spell_ordinal(10, false), "dixième");
        assert_eq!(spell_ordinal(21, false), "vingt et unième");
        // Le féminin ne se distingue qu'au premier rang.
        assert_eq!(spell_ordinal(2, true), "deuxième");
    }

    #[test]
    fn drops_the_trailing_e_before_ieme() {
        assert_eq!(spell_ordinal(4, false), "quatrième");
        assert_eq!(spell_ordinal(11, false), "onzième");
        assert_eq!(spell_ordinal(30, false), "trentième");
        assert_eq!(spell_ordinal(1000, false), "millième");
    }

    #[test]
    fn repairs_cinq_and_neuf_before_ieme() {
        assert_eq!(spell_ordinal(5, false), "cinquième");
        assert_eq!(spell_ordinal(9, false), "neuvième");
        assert_eq!(spell_ordinal(19, false), "dix-neuvième");
    }

    #[test]
    fn drops_the_plural_mark_before_ieme() {
        // « quatre-vingts » perd son s dès qu'il est suivi : « quatre-vingtième ».
        assert_eq!(spell_ordinal(80, false), "quatre-vingtième");
        assert_eq!(spell_ordinal(200, false), "deux centième");
    }

    #[test]
    fn spells_units_and_teens() {
        assert_eq!(spell_cardinal(0), "zéro");
        assert_eq!(spell_cardinal(1), "un");
        assert_eq!(spell_cardinal(7), "sept");
        assert_eq!(spell_cardinal(11), "onze");
        assert_eq!(spell_cardinal(16), "seize");
        assert_eq!(spell_cardinal(17), "dix-sept");
    }

    #[test]
    fn joins_tens_and_units_with_et_on_ones() {
        assert_eq!(spell_cardinal(20), "vingt");
        assert_eq!(spell_cardinal(21), "vingt et un");
        assert_eq!(spell_cardinal(22), "vingt-deux");
        assert_eq!(spell_cardinal(31), "trente et un");
        assert_eq!(spell_cardinal(45), "quarante-cinq");
    }

    #[test]
    fn counts_seventies_and_nineties_by_sixteens() {
        assert_eq!(spell_cardinal(70), "soixante-dix");
        assert_eq!(spell_cardinal(71), "soixante et onze");
        assert_eq!(spell_cardinal(75), "soixante-quinze");
        assert_eq!(spell_cardinal(90), "quatre-vingt-dix");
        assert_eq!(spell_cardinal(91), "quatre-vingt-onze");
        assert_eq!(spell_cardinal(99), "quatre-vingt-dix-neuf");
    }

    #[test]
    fn pluralises_quatre_vingts_only_when_nothing_follows() {
        assert_eq!(spell_cardinal(80), "quatre-vingts");
        assert_eq!(spell_cardinal(81), "quatre-vingt-un");
        assert_eq!(spell_cardinal(180), "cent quatre-vingts");
        assert_eq!(spell_cardinal(189), "cent quatre-vingt-neuf");
    }

    #[test]
    fn pluralises_cents_only_when_nothing_follows() {
        assert_eq!(spell_cardinal(100), "cent");
        assert_eq!(spell_cardinal(101), "cent un");
        assert_eq!(spell_cardinal(200), "deux cents");
        assert_eq!(spell_cardinal(201), "deux cent un");
        assert_eq!(spell_cardinal(999), "neuf cent quatre-vingt-dix-neuf");
    }

    #[test]
    fn keeps_mille_invariable_and_implicit() {
        assert_eq!(spell_cardinal(1000), "mille");
        assert_eq!(spell_cardinal(1001), "mille un");
        assert_eq!(spell_cardinal(2000), "deux mille");
        assert_eq!(spell_cardinal(80000), "quatre-vingt mille");
    }

    #[test]
    fn spells_years_as_whole_numbers() {
        assert_eq!(spell_cardinal(1789), "mille sept cent quatre-vingt-neuf");
        assert_eq!(spell_cardinal(1999), "mille neuf cent quatre-vingt-dix-neuf");
        assert_eq!(spell_cardinal(2026), "deux mille vingt-six");
        assert_eq!(spell_cardinal(2029), "deux mille vingt-neuf");
    }
}
