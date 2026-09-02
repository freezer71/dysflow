# 2. Démonstration de la vue

Une note tenue sur trois lignes courtes,
telles qu'elles sont dans le fichier,
sans rien de recomposé.

Un mot **en gras,** un autre en *italique,* un [lien](https://example.org),
du `code` au fil du texte, et du ~~barré~~ pour finir.

| Route       | Méthode | Réponse |
|-------------|---------|---------|
| /users      | GET     | 200     |
| /users/{id} | DELETE  | 204     |

- Un item court
- Un item assez long pour déborder de la colonne et se replier proprement sous son propre texte
  - Un item imbriqué

> Une citation tenue
> sur deux lignes.

```rust
fn lire(texte: &str) -> Voix {
    // Le code est annoncé, jamais récité ; il reste affiché tel qu'il est écrit.
    Voix::depuis(texte)
}
```

## 2.1 Ce que l'œil suit

Le mot prononcé porte un fond, la ligne qui le tient porte la règle de lecture.
Hors du bloc lu, le texte s'efface sans disparaître : le gras reste du gras.

Un dernier paragraphe, pour que la fin du document se voie venir.
