import CoreGraphics

/// Le trait sous la ligne lue (« encre calme ») : du début de la ligne au
/// point où en est la voix dans le mot.
enum CursorLine {
    static let thickness: CGFloat = 2
    /// Haut du trait, au-dessus du bas du mot : juste sous la ligne de
    /// base, là où passerait un soulignement (le bas du mot est celui de ses
    /// jambages).
    static let lift: CGFloat = 3

    /// Le trait pour `word` (son cadre à l'écran), sur une ligne qui commence
    /// en `lineStartX`, quand la voix a dit `progress` du mot (0 à 1). Sans
    /// mouvement (Réduire les animations), il avance mot par mot : le point
    /// est la fin du mot lu.
    static func rect(word: CGRect, lineStartX: CGFloat, progress: CGFloat,
                     reduceMotion: Bool) -> CGRect {
        let p = reduceMotion ? 1 : (progress.isFinite ? min(max(progress, 0), 1) : 0)
        let point = word.minX + word.width * p
        let start = min(lineStartX, word.minX)
        return CGRect(x: start, y: word.maxY - lift,
                      width: max(0, point - start), height: thickness)
    }
}
