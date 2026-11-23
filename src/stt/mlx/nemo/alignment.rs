//! Jetons datés, phrases et recollage des morceaux d'un long audio, sans
//! MLX. Port de `NemoAlignment.swift`.

/// Un jeton reconnu, daté en secondes.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedToken {
    pub id: usize,
    pub text: String,
    pub start: f64,
    pub duration: f64,
}

impl AlignedToken {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
}

/// Une phrase : son texte et ses jetons, rangés par début (tri stable,
/// comme `sorted` du Swift sur des débuts déjà croissants).
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedSentence {
    pub text: String,
    pub tokens: Vec<AlignedToken>,
}

impl AlignedSentence {
    pub fn new(text: String, mut tokens: Vec<AlignedToken>) -> Self {
        tokens.sort_by(|a, b| a.start.total_cmp(&b.start));
        Self { text, tokens }
    }

    pub fn start(&self) -> f64 {
        self.tokens.first().map_or(0.0, |t| t.start)
    }

    pub fn end(&self) -> f64 {
        self.tokens.last().map_or(0.0, AlignedToken::end)
    }
}

/// Le résultat d'une lecture : le texte, sans espaces autour, et ses
/// phrases.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedResult {
    pub text: String,
    pub sentences: Vec<AlignedSentence>,
}

impl AlignedResult {
    pub fn new(text: &str, sentences: Vec<AlignedSentence>) -> Self {
        Self {
            text: text.trim().to_string(),
            sentences,
        }
    }

    /// Les jetons de toutes les phrases, dans leur ordre.
    pub fn tokens(&self) -> impl Iterator<Item = &AlignedToken> {
        self.sentences.iter().flat_map(|s| s.tokens.iter())
    }
}

/// Les jetons groupés en phrases : une phrase finit sur `!`, `?`, `。`,
/// `？`, `！`, ou sur un `.` suivi d'un jeton qui commence un mot (ou de
/// rien).
pub fn tokens_to_sentences(tokens: &[AlignedToken]) -> Vec<AlignedSentence> {
    let mut sentences = Vec::new();
    let mut current: Vec<AlignedToken> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        current.push(token.clone());
        if closes_sentence(token, index, tokens) {
            let text: String = current.iter().map(|t| t.text.as_str()).collect();
            sentences.push(AlignedSentence::new(text, std::mem::take(&mut current)));
        }
    }
    if !current.is_empty() {
        let text: String = current.iter().map(|t| t.text.as_str()).collect();
        sentences.push(AlignedSentence::new(text, current));
    }
    sentences
}

/// Les phrases en résultat : leurs textes à la suite.
pub fn sentences_to_result(sentences: Vec<AlignedSentence>) -> AlignedResult {
    let text: String = sentences.iter().map(|s| s.text.as_str()).collect();
    AlignedResult::new(&text, sentences)
}

/// Pas assez de jetons communs pour recoller par la plus longue suite
/// contiguë.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoStrongOverlap;

/// Recolle deux morceaux qui se chevauchent par leur plus longue suite
/// contiguë de jetons communs (même jeton, débuts à moins de la moitié du
/// chevauchement). Trop peu de paires : `NoStrongOverlap`.
pub fn merge_longest_contiguous(
    a: &[AlignedToken],
    b: &[AlignedToken],
    overlap: f64,
) -> Result<Vec<AlignedToken>, NoStrongOverlap> {
    let Some((overlap_a, overlap_b)) = overlaps(a, b, overlap) else {
        return Ok(concat_or_cut(a, b));
    };
    let enough_pairs = overlap_a.len() / 2;
    if overlap_a.len() < 2 || overlap_b.len() < 2 {
        return Ok(cut(a, b));
    }
    let mut best: Vec<(usize, usize)> = Vec::new();
    for i in 0..overlap_a.len() {
        for j in 0..overlap_b.len() {
            if !matches(overlap_a[i], overlap_b[j], overlap / 2.0) {
                continue;
            }
            let mut chain = Vec::new();
            let (mut k, mut l) = (i, j);
            while k < overlap_a.len()
                && l < overlap_b.len()
                && matches(overlap_a[k], overlap_b[l], overlap / 2.0)
            {
                chain.push((k, l));
                k += 1;
                l += 1;
            }
            if chain.len() > best.len() {
                best = chain;
            }
        }
    }
    if best.len() < enough_pairs || best.is_empty() {
        return Err(NoStrongOverlap);
    }
    Ok(stitch(a, b, overlap_a.len(), &best))
}

/// Recolle deux morceaux par leur plus longue sous-suite commune ; aucune
/// paire : coupés au milieu du chevauchement.
pub fn merge_longest_common_subsequence(
    a: &[AlignedToken],
    b: &[AlignedToken],
    overlap: f64,
) -> Vec<AlignedToken> {
    let Some((overlap_a, overlap_b)) = overlaps(a, b, overlap) else {
        return concat_or_cut(a, b);
    };
    if overlap_a.len() < 2 || overlap_b.len() < 2 {
        return cut(a, b);
    }
    let (rows, cols) = (overlap_a.len() + 1, overlap_b.len() + 1);
    let mut dp = vec![vec![0usize; cols]; rows];
    for i in 1..rows {
        for j in 1..cols {
            dp[i][j] = if matches(overlap_a[i - 1], overlap_b[j - 1], overlap / 2.0) {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (overlap_a.len(), overlap_b.len());
    while i > 0 && j > 0 {
        if matches(overlap_a[i - 1], overlap_b[j - 1], overlap / 2.0) {
            pairs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs.reverse();
    if pairs.is_empty() {
        return cut(a, b);
    }
    stitch(a, b, overlap_a.len(), &pairs)
}

/// Les jetons de `a` qui finissent dans le chevauchement et ceux de `b` qui
/// y commencent ; `None` si l'un est vide ou s'ils ne se chevauchent pas.
fn overlaps<'a>(
    a: &'a [AlignedToken],
    b: &'a [AlignedToken],
    overlap: f64,
) -> Option<(Vec<&'a AlignedToken>, Vec<&'a AlignedToken>)> {
    let (last, first) = (a.last()?, b.first()?);
    let (a_end, b_start) = (last.end(), first.start);
    if a_end <= b_start {
        return None;
    }
    Some((
        a.iter().filter(|t| t.end() > b_start - overlap).collect(),
        b.iter().filter(|t| t.start < a_end + overlap).collect(),
    ))
}

/// `a` puis `b` (l'un vide, ou sans chevauchement).
fn concat_or_cut(a: &[AlignedToken], b: &[AlignedToken]) -> Vec<AlignedToken> {
    a.iter().chain(b).cloned().collect()
}

/// Coupés au milieu du chevauchement.
fn cut(a: &[AlignedToken], b: &[AlignedToken]) -> Vec<AlignedToken> {
    let (a_end, b_start) = (
        a.last().map_or(0.0, AlignedToken::end),
        b.first().map_or(0.0, |t| t.start),
    );
    let cutoff = (a_end + b_start) / 2.0;
    a.iter()
        .filter(|t| t.end() <= cutoff)
        .chain(b.iter().filter(|t| t.start >= cutoff))
        .cloned()
        .collect()
}

/// `a` jusqu'à la première paire, chaque paire (le jeton de `a`, puis le
/// plus long des deux trous), puis `b` après la dernière. Les paires
/// indexent la fin de `a` (`overlap_a` jetons) et le début de `b`.
fn stitch(
    a: &[AlignedToken],
    b: &[AlignedToken],
    overlap_a: usize,
    pairs: &[(usize, usize)],
) -> Vec<AlignedToken> {
    let a_start = a.len() - overlap_a;
    let indices_a: Vec<usize> = pairs.iter().map(|p| a_start + p.0).collect();
    let indices_b: Vec<usize> = pairs.iter().map(|p| p.1).collect();
    let mut merged: Vec<AlignedToken> = a[..indices_a[0]].to_vec();
    for idx in 0..pairs.len() {
        let (ia, ib) = (indices_a[idx], indices_b[idx]);
        merged.push(a[ia].clone());
        if idx + 1 < pairs.len() {
            let gap_a = &a[ia + 1..indices_a[idx + 1]];
            let gap_b = &b[ib + 1..indices_b[idx + 1]];
            merged.extend_from_slice(if gap_b.len() > gap_a.len() {
                gap_b
            } else {
                gap_a
            });
        }
    }
    merged.extend_from_slice(&b[indices_b[indices_b.len() - 1] + 1..]);
    merged
}

fn closes_sentence(token: &AlignedToken, index: usize, all: &[AlignedToken]) -> bool {
    let text = &token.text;
    if ['!', '?', '。', '？', '！']
        .iter()
        .any(|c| text.contains(*c))
    {
        return true;
    }
    if text.contains('.') {
        return all
            .get(index + 1)
            .is_none_or(|next| next.text.contains(' '));
    }
    false
}

fn matches(lhs: &AlignedToken, rhs: &AlignedToken, overlap: f64) -> bool {
    lhs.id == rhs.id && (lhs.start - rhs.start).abs() < overlap
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(id: usize, text: &str, start: f64) -> AlignedToken {
        AlignedToken {
            id,
            text: text.to_string(),
            start,
            duration: 0.08,
        }
    }

    #[test]
    fn phrases_coupees_sur_la_ponctuation() {
        let tokens = [
            token(1, " Bon", 0.0),
            token(2, "jour", 0.1),
            token(3, ".", 0.2),
            token(4, " Ça", 0.3),
            token(5, " va", 0.4),
            token(6, "?", 0.5),
            token(7, " v1", 0.6),
            token(8, ".", 0.7),
            token(9, "2", 0.8),
        ];
        let sentences = tokens_to_sentences(&tokens);
        let texts: Vec<&str> = sentences.iter().map(|s| s.text.as_str()).collect();
        // Un point suivi d'un jeton collé (« v1.2 ») ne ferme pas la phrase.
        assert_eq!(texts, [" Bonjour.", " Ça va?", " v1.2"]);
        let result = sentences_to_result(sentences);
        assert_eq!(result.text, "Bonjour. Ça va? v1.2");
        assert_eq!(result.tokens().count(), 9);
    }

    #[test]
    fn tri_stable_par_debut() {
        let s = AlignedSentence::new(
            "x".into(),
            vec![token(2, "b", 0.1), token(1, "a", 0.1), token(0, "c", 0.0)],
        );
        let ids: Vec<usize> = s.tokens.iter().map(|t| t.id).collect();
        assert_eq!(ids, [0, 2, 1]);
        assert_eq!((s.start(), s.end()), (0.0, 0.1 + 0.08));
    }

    #[test]
    fn recollage_sans_chevauchement() {
        let a = [token(1, "a", 0.0)];
        let b = [token(2, "b", 1.0)];
        assert_eq!(merge_longest_contiguous(&a, &b, 2.0).unwrap().len(), 2);
        assert_eq!(merge_longest_common_subsequence(&[], &b, 2.0), b.to_vec());
    }

    #[test]
    fn recollage_par_la_suite_commune() {
        let a: Vec<_> = (0..6).map(|i| token(10 + i, "x", i as f64 * 0.5)).collect();
        // `b` reprend à 1,5 s les jetons 13, 14, 15 puis continue.
        let b: Vec<_> = (3..9)
            .map(|i| token(10 + i, "x", i as f64 * 0.5 + 0.05))
            .collect();
        let merged = merge_longest_contiguous(&a, &b, 2.0).unwrap();
        let ids: Vec<usize> = merged.iter().map(|t| t.id).collect();
        assert_eq!(ids, (10..19).collect::<Vec<_>>());
        let lcs = merge_longest_common_subsequence(&a, &b, 2.0);
        assert_eq!(lcs.iter().map(|t| t.id).collect::<Vec<_>>(), ids);
    }

    #[test]
    fn chevauchement_sans_paires() {
        let a: Vec<_> = (0..4).map(|i| token(i, "x", i as f64 * 0.5)).collect();
        let b: Vec<_> = (0..4)
            .map(|i| token(100 + i, "y", 1.0 + i as f64 * 0.5))
            .collect();
        assert_eq!(merge_longest_contiguous(&a, &b, 2.0), Err(NoStrongOverlap));
        let cut = merge_longest_common_subsequence(&a, &b, 2.0);
        assert!(cut
            .iter()
            .all(|t| t.id < 100 && t.end() <= 1.29 || t.id >= 100 && t.start >= 1.29));
    }
}
