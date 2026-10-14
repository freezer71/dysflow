/// Les enregistrements du pont que les réglages font passer d'une tâche à
/// l'autre. Ce sont des valeurs (chaînes, nombres, tableaux d'autres
/// enregistrements), copiées à chaque passage ; la conformité ne peut être
/// vérifiée par le compilateur que dans le fichier généré, d'où `@unchecked`.
extension SettingsView: @unchecked Sendable {}
extension SettingsPatch: @unchecked Sendable {}
extension LanguageVoice: @unchecked Sendable {}
