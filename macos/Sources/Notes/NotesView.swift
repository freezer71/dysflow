import SwiftUI

/// La fenêtre des notes, à la manière de Notion : les pages à gauche, la
/// page ouverte à droite — un grand titre, puis le texte, qu'on tape ou
/// qu'on colle. « Lire » l'envoie au lecteur.
struct NotesView: View {
    let store: NotesStore
    /// Lire une page : le panneau s'ouvre sur elle.
    let read: (NotePage) -> Void
    @State private var selection: String?

    var body: some View {
        HStack(spacing: 0) {
            NotesSidebar(store: store, selection: $selection)
            Rectangle().fill(Palette.divider).frame(width: 1).accessibilityHidden(true)
            Group {
                if let id = selection, store.page(id) != nil {
                    NoteEditor(store: store, id: id, read: read)
                        .id(id)
                } else {
                    emptyState
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Palette.page)
        }
        .ignoresSafeArea()
        .frame(minWidth: 640, minHeight: 420)
        .font(Typo.body)
        .onAppear {
            // Rien encore : une page vierge attend qu'on écrive.
            if store.pages.isEmpty { store.create() }
            if selection == nil { selection = store.pages.first?.id }
        }
        .accessibilityIdentifier("notes")
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            LogoMark().frame(width: 32, height: 32).foregroundStyle(Palette.faint)
            Button("Nouvelle page") { selection = store.create().id }
                .buttonStyle(.notion)
        }
    }
}

/// Les pages, comme la barre latérale de Notion.
private struct NotesSidebar: View {
    let store: NotesStore
    @Binding var selection: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 8) {
                LogoMark(small: true).frame(width: 18, height: 18).foregroundStyle(Palette.ink)
                Text("sillon").font(.system(size: 14, weight: .semibold)).foregroundStyle(Palette.ink)
            }
            .padding(.horizontal, 8)
            .padding(.top, 48)
            .padding(.bottom, 14)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("sillon")

            SidebarRow(symbol: "square.and.pencil", title: "Nouvelle page", selected: false) {
                selection = store.create().id
            }
            .keyboardShortcut("n", modifiers: .command)
            .accessibilityIdentifier("nouvelle-page")

            Text("Pages")
                .font(Typo.caption)
                .foregroundStyle(Palette.muted)
                .padding(.horizontal, 8)
                .padding(.top, 16)
                .padding(.bottom, 4)
                .accessibilityAddTraits(.isHeader)

            ScrollView {
                VStack(alignment: .leading, spacing: 1) {
                    ForEach(store.pages) { page in
                        SidebarRow(symbol: "doc.text", title: page.displayTitle,
                                   selected: selection == page.id) {
                            selection = page.id
                        }
                        .contextMenu {
                            Button("Supprimer la page") { delete(page.id) }
                        }
                    }
                }
            }
            .scrollIndicators(.never)
        }
        .padding(.horizontal, 8)
        .frame(width: 230)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Palette.sidebar)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Pages")
    }

    private func delete(_ id: String) {
        let index = store.pages.firstIndex { $0.id == id }
        store.delete(id)
        guard selection == id else { return }
        // La page voisine prend la place, comme dans Notion.
        let next = index.flatMap { store.pages.indices.contains($0) ? $0 : store.pages.indices.last }
        selection = next.map { store.pages[$0].id }
    }
}

/// Une ligne de la barre latérale : icône, titre, fond au survol.
private struct SidebarRow: View {
    let symbol: String
    let title: String
    let selected: Bool
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 9) {
                Image(systemName: symbol)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(selected ? Palette.ink : Palette.faint)
                    .frame(width: 18)
                Text(title)
                    .font(.system(size: 13, weight: selected ? .semibold : .medium))
                    .foregroundStyle(selected ? Palette.ink : Palette.muted)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .frame(height: 30)
            .background(RoundedRectangle(cornerRadius: 6)
                .fill(selected ? Palette.selected : (hovering ? Palette.hover : .clear)))
            .contentShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .accessibilityLabel(title)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// La page ouverte : un grand titre, puis le texte libre.
private struct NoteEditor: View {
    let store: NotesStore
    let id: String
    let read: (NotePage) -> Void
    @State private var title = ""
    @State private var text = ""
    @FocusState private var focus: Field?

    private enum Field { case title, body }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Spacer()
                Button {
                    store.flush()
                    if let page = store.page(id) { read(page) }
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "play.fill").font(.system(size: 10))
                        Text("Lire")
                    }
                }
                .buttonStyle(.notionPrimary)
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(title.isEmpty && text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .help("Lire cette page (⌘↩)")
                .accessibilityIdentifier("lire-page")
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
            .frame(height: 52)

            TextField("Sans titre", text: $title)
                .textFieldStyle(.plain)
                .font(.system(size: 34, weight: .bold))
                .foregroundStyle(Palette.ink)
                .focused($focus, equals: .title)
                .onSubmit { focus = .body }
                .padding(.horizontal, 56)
                .padding(.top, 28)
                .padding(.bottom, 12)
                .accessibilityLabel("Titre de la page")
                .accessibilityIdentifier("titre-page")

            ZStack(alignment: .topLeading) {
                if text.isEmpty {
                    Text("Écrivez, ou collez un texte avec ⌘V…")
                        .font(.system(size: 16))
                        .foregroundStyle(Palette.faint)
                        .padding(.leading, 5)
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
                }
                TextEditor(text: $text)
                    .font(.system(size: 16))
                    .lineSpacing(6)
                    .foregroundStyle(Palette.ink)
                    .scrollContentBackground(.hidden)
                    .focused($focus, equals: .body)
                    .accessibilityLabel("Texte de la page")
                    .accessibilityIdentifier("texte-page")
            }
            .padding(.horizontal, 51)
            .padding(.bottom, 24)
        }
        .onAppear {
            guard let page = store.page(id) else { return }
            title = page.title
            text = page.body
            // Une page vierge : on commence par le titre ; sinon par le texte.
            DispatchQueue.main.async { focus = page.title.isEmpty && page.body.isEmpty ? .title : .body }
        }
        .onChange(of: title) { _, value in store.update(id, title: value) }
        .onChange(of: text) { _, value in store.update(id, body: value) }
    }
}
