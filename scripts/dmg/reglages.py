# Les réglages de dmgbuild pour le .dmg de Dysflow (scripts/package-macos.sh).
# dmgbuild écrit lui-même le .DS_Store de la fenêtre : pas de Finder à
# piloter, la mise en page tient aussi sur la CI.
#
#   dmgbuild -s scripts/dmg/reglages.py -D app=… -D fond=… -D icone=… "Dysflow 0.7.0" sortie.dmg

import os.path

app = defines["app"]  # noqa: F821 (fourni par dmgbuild)
nom = os.path.basename(app)

format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = defines["icone"]  # noqa: F821 — l'icône du volume monté

# La fenêtre : le fond de 640 × 400, plus les 32 points de la barre de
# titre que dmgbuild compte dans la hauteur. Sans barre d'outils ni d'état.
background = defines["fond"]  # noqa: F821 — fond.png ; fond@2x.png à côté
window_rect = ((200, 160), (640, 432))
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
default_view = "icon-view"

# Les icônes : les centres de scripts/dmg/fond.py.
icon_size = 128
text_size = 14
icon_locations = {nom: (170, 165), "Applications": (470, 165)}
