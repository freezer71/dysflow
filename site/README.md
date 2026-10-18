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
`src/models.rs` et les mesures du README (à mettre à jour à chaque nouveau
modèle). `liens.ts` n'a jamais à changer : les liens pointent vers
`/releases/latest/download/…`, et le numéro de version est lu sur GitHub
(gardé une heure).
