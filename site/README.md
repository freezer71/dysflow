# Le site de sillon

Site de présentation, en Next.js : l'accueil (avantages, comparaison,
questions), le catalogue des modèles (`/modeles`) et le téléchargement
(`/telecharger`). Toutes les pages sont statiques.

```bash
npm install
npm run dev      # http://localhost:3000
npm run build    # compile et prérend les pages
```

Les données vivent dans `app/_data/` : `modeles.ts` reprend le catalogue de
`src/models.rs` et les mesures du README, `liens.ts` la version publiée et
ses archives. À mettre à jour à chaque nouveau modèle ou nouvelle version.
