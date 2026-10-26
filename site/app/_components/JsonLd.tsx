/**
 * Des données structurées (schema.org) pour les moteurs de recherche et les
 * assistants. `<` est échappé : un texte ne peut pas fermer la balise.
 */
export function JsonLd({ donnees }: { donnees: object }) {
  return (
    <script
      type="application/ld+json"
      dangerouslySetInnerHTML={{ __html: JSON.stringify(donnees).replace(/</g, "\\u003c") }}
    />
  );
}
