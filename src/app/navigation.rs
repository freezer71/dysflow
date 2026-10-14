//! Se repérer dans le texte : vue, recherche, reprise, signets, panneaux.

use super::*;

impl App {
    /// Bloc autour duquel dessiner, lecture ou vue figée.
    pub fn anchor_block(&self) -> usize {
        let current = self
            .doc()
            .sentence(self.current_sentence())
            .map(|s| s.block_id)
            .unwrap_or(0);
        self.view.block(current)
    }

    /// Fait défiler la vue de `delta` blocs, sans toucher à la lecture.
    pub fn scroll_view(&mut self, delta: isize) {
        let current = self
            .doc()
            .sentence(self.current_sentence())
            .map(|s| s.block_id)
            .unwrap_or(0);
        let last = self.doc().blocks.len().saturating_sub(1);
        self.view.scroll(delta, current, last);
        self.notify("Vue figée — f revient à la lecture");
    }

    pub fn follow_reading(&mut self) {
        self.view.follow();
        self.notify("Retour à la lecture");
    }

    pub fn toggle_follow(&mut self) {
        if self.view.is_following() {
            self.scroll_view(0);
        } else {
            self.follow_reading();
        }
    }

    pub fn search(&self) -> Option<&Search> {
        self.search.as_ref()
    }

    /// Requête en cours de saisie, s'il y en a une.
    pub fn search_input(&self) -> Option<&str> {
        self.search_input.as_deref()
    }

    pub fn search_open(&mut self) {
        self.search_input = Some(String::new());
    }

    pub fn search_push(&mut self, c: char) {
        if let Some(buffer) = self.search_input.as_mut() {
            buffer.push(c);
        }
    }

    pub fn search_backspace(&mut self) {
        if let Some(buffer) = self.search_input.as_mut() {
            buffer.pop();
        }
    }

    pub fn search_cancel(&mut self) {
        self.search_input = None;
        self.search = None;
    }

    /// Valide la recherche et envoie la lecture sur le premier résultat.
    ///
    /// Sans résultat, rien ne bouge : déplacer la lecture au hasard ferait
    /// perdre la page en cours pour rien.
    pub fn search_commit(&mut self) {
        let Some(query) = self.search_input.take() else {
            return;
        };
        let from = self.current_sentence();
        let search = Search::new(self.doc(), &query, from);
        if search.is_empty() {
            self.search = None;
            self.notify(&format!("Aucun résultat pour « {} »", query.trim()));
            return;
        }
        let target = search.target();
        self.search = Some(search);
        if let Some(target) = target {
            self.seek_to(target);
        }
        // Le rang des résultats est porté en permanence par la ligne d'état :
        // le répéter ici le dirait deux fois sur la même ligne.
    }

    /// Passe au résultat suivant ou précédent.
    pub fn search_advance(&mut self, delta: isize) {
        let Some(search) = self.search.as_mut() else {
            self.notify("Aucune recherche en cours — / pour en lancer une");
            return;
        };
        if let Some(target) = search.advance(delta) {
            self.seek_to(target);
        }
    }

    /// Le document tel qu'il se nomme à l'écran : son nom de fichier.
    ///
    /// L'identité, elle, est le chemin entier ; il déborderait de l'en-tête.
    pub fn source_label(&self) -> String {
        std::path::Path::new(&self.source)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.source.clone())
    }

    /// Rattache à ce document ce qui a été retenu sous un ancien nom.
    ///
    /// Positions et signets étaient rattachés au nom du fichier seul ; ils le
    /// sont désormais à son chemin. Rien de ce qui a été posé ne se perd.
    pub fn adopt_former_identity(&mut self, former: &str) {
        self.positions.adopt(former, &self.source);
        self.bookmarks.adopt(former, &self.source);
    }

    /// Retient la position dans ce document, et reprend où l'on s'était arrêté.
    ///
    /// Réservé aux documents qui ont une identité stable : un flux et un
    /// collage ne sont pas les mêmes d'une fois sur l'autre, et leur rendre
    /// une position ancienne enverrait la lecture n'importe où.
    pub fn resume_where_left(&mut self) {
        self.remembers_place = true;
        let Some((at, snippet)) = self
            .positions
            .get(&self.source)
            .map(|p| (p.sentence_id, p.snippet.clone()))
        else {
            return;
        };
        // À l'index retenu si la phrase y est encore ; sinon là où elle est
        // passée. Un paragraphe ajouté en tête décale tout, et la reprise
        // tomberait au milieu d'un autre passage.
        let target = {
            let doc = self.doc();
            if doc.sentence(at).map(|s| s.written()).as_deref() == Some(snippet.as_str()) {
                Some(at)
            } else {
                doc.sentences.iter().position(|s| s.written() == snippet)
            }
        };
        // Reprendre au début n'est pas une reprise : rien à dire, rien à faire.
        let Some(target) = target.filter(|t| *t > 0) else {
            return;
        };
        self.seek_to(target);
        self.notify("Reprise là où vous en étiez — g revient au début");
    }

    /// Où en est la lecture, si cela vaut d'être retenu.
    ///
    /// Rendue plutôt qu'écrite : c'est la décision qui se vérifie, l'écriture
    /// n'ayant lieu qu'à la fermeture.
    pub fn place_to_remember(&self) -> Option<(usize, String)> {
        if !self.remembers_place {
            return None;
        }
        let id = self.current_sentence();
        // Un document lu jusqu'au bout n'a plus de position : le rouvrir doit
        // le reprendre au début, non sur sa dernière phrase.
        if id + 1 >= self.doc().sentences.len() {
            return None;
        }
        Some((id, self.doc().sentence(id)?.written()))
    }

    /// Note où en était la lecture, dans la mémoire des positions.
    ///
    /// Sans écrire : l'écriture a lieu à la fermeture, une fois pour tous les
    /// documents lus dans la session.
    pub(super) fn note_position(&mut self) {
        if !self.remembers_place {
            return;
        }
        match self.place_to_remember() {
            Some((id, snippet)) => self.positions.record(&self.source, id, &snippet),
            None => self.positions.forget(&self.source),
        }
    }

    /// Note où en était la lecture et l'écrit, pour la prochaine ouverture.
    pub fn remember_position(&mut self) {
        if !self.remembers_place {
            return;
        }
        self.note_position();
        // La fermeture est en cours : plus personne n'est là pour lire une
        // erreur, et une position perdue coûte un défilement.
        let _ = self.positions.save();
    }

    pub fn toggle_bookmark(&mut self) {
        let sentence_id = self.current_sentence();
        let Some(sentence) = self.doc().sentence(sentence_id) else {
            return;
        };
        let block_id = sentence.block_id;
        // Un signet cite ce qu'on a sous les yeux, pas ce que le moteur entend.
        let snippet = sentence.written();
        let title = self
            .doc()
            .section_of_sentence(sentence_id)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "Sans titre".to_string());

        let added = self
            .bookmarks
            .toggle(&self.source, sentence_id, block_id, &title, &snippet);
        match self.bookmarks.save() {
            Ok(()) => self.notify(if added { "Signet ajouté" } else { "Signet retiré" }),
            Err(err) => self.notify(&format!("Signet non enregistré : {err}")),
        }
    }

    pub fn bookmarked_here(&self) -> bool {
        self.bookmarks.contains(&self.source, self.current_sentence())
    }

    pub fn toggle_panel(&mut self, panel: Panel) {
        self.panel = if self.panel == panel { Panel::None } else { panel };
        self.selection = 0;
    }

    /// Sections atteignables, dans l'ordre du document.
    ///
    /// La section racine reste vide quand le document ouvre sur un titre ;
    /// l'exclure garde l'index de sélection aligné sur ce qui est affiché.
    pub fn navigable_sections(&self) -> Vec<usize> {
        self.doc()
            .sections
            .iter()
            .filter(|s| s.first_sentence.is_some())
            .map(|s| s.id)
            .collect()
    }

    /// Déplace la sélection dans le panneau ouvert.
    pub fn move_selection(&mut self, delta: isize) {
        let count = match self.panel {
            Panel::Contents => self.navigable_sections().len(),
            Panel::Bookmarks => self.bookmarks.for_document(&self.source).count(),
            _ => 0,
        };
        if count == 0 {
            return;
        }
        let next = (self.selection as isize + delta).clamp(0, count as isize - 1);
        self.selection = next as usize;
    }

    /// Saute à l'élément sélectionné dans le panneau.
    pub fn activate_selection(&mut self) {
        match self.panel {
            Panel::Contents => {
                let target = self
                    .navigable_sections()
                    .get(self.selection)
                    .and_then(|&id| self.doc().section(id))
                    .and_then(|s| s.first_sentence);
                if let Some(target) = target {
                    self.seek_to(target);
                    self.notify("Saut au titre");
                }
            }
            Panel::Bookmarks => {
                let target = self
                    .bookmarks
                    .for_document(&self.source)
                    .nth(self.selection)
                    .map(|b| b.sentence_id);
                if let Some(target) = target {
                    self.seek_to(target);
                    self.notify("Saut au signet");
                }
            }
            _ => {}
        }
    }

    pub fn delete_selection(&mut self) {
        if self.panel != Panel::Bookmarks {
            return;
        }
        let target = self
            .bookmarks
            .for_document(&self.source)
            .nth(self.selection)
            .map(|b| b.sentence_id);
        if let Some(sentence_id) = target {
            self.bookmarks.remove(&self.source, sentence_id);
            let _ = self.bookmarks.save();
            self.selection = self.selection.saturating_sub(1);
            self.notify("Signet supprimé");
        }
    }

    // ----------------------------------------------------- écran de configuration
}
