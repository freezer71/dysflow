# Fichier de chaîne d'outils (toolchain) de la copie Rust de MLX : il range les
# cinq sources que mlx-sys (mlx-c, CMake FetchContent) irait chercher sur le
# réseau, et refuse toute autre. Les sources viennent de scripts/mlx-sources.sh
# (commits et empreintes épinglés) ; ce fichier ne télécharge rien.
#
# Il fixe aussi la cible macOS : celle de MACOSX_DEPLOYMENT_TARGET à chaque
# configuration, pas celle de la première, que CMake garderait en cache.
#
# Il n'est lu que par CMake, par la variable d'environnement
# CMAKE_TOOLCHAIN_FILE_aarch64_apple_darwin que pose mlx-sources.sh. Il ne pose
# pas CMAKE_SYSTEM_NAME : ce n'est pas une compilation croisée.

get_filename_component(_sources "${CMAKE_CURRENT_LIST_DIR}/../target/mlx-sources" ABSOLUTE)

# Chaque variable doit désigner un dossier présent : sans lui, FetchContent
# retomberait sur son téléchargement.
foreach(_dep MLX FMT GGUFLIB JSON METAL_CPP)
  string(TOLOWER "${_dep}" _dir)
  if(NOT IS_DIRECTORY "${_sources}/${_dir}")
    message(FATAL_ERROR "Source de MLX manquante : ${_sources}/${_dir} (. scripts/mlx-sources.sh)")
  endif()
  set(FETCHCONTENT_SOURCE_DIR_${_dep} "${_sources}/${_dir}" CACHE PATH "source épinglée" FORCE)
endforeach()

# Une source qui ne serait pas dans la liste ci-dessus ferait échouer la
# configuration au lieu de se télécharger.
set(FETCHCONTENT_FULLY_DISCONNECTED ON CACHE BOOL "aucune source hors des épingles" FORCE)

# La cible du moment (vide : celle de ce Mac). MLX en tire ses noyaux (NAX dès
# 26.2) ; mlx-sys reprend son dossier de build d'une cible à l'autre, et CMake
# y recompile bibliothèques et noyaux dès que cette valeur change.
set(CMAKE_OSX_DEPLOYMENT_TARGET "$ENV{MACOSX_DEPLOYMENT_TARGET}" CACHE STRING "cible macOS" FORCE)

# Un MLX_RS_METAL_PATH resté d'une autre cible écraserait ses noyaux.
if(DEFINED MLX_METAL_PATH)
  get_filename_component(_noyaux "${CMAKE_CURRENT_LIST_DIR}/../target/mlx-metallib" ABSOLUTE)
  get_filename_component(_sortie "${MLX_METAL_PATH}" ABSOLUTE)
  set(_attendu "${_noyaux}")
  if(NOT "$ENV{MACOSX_DEPLOYMENT_TARGET}" STREQUAL "")
    set(_attendu "${_noyaux}/macos-$ENV{MACOSX_DEPLOYMENT_TARGET}")
  endif()
  string(FIND "${_sortie}/" "${_noyaux}/" _dedans)
  if(_dedans EQUAL 0 AND NOT _sortie STREQUAL _attendu)
    message(FATAL_ERROR "${_sortie} n'est pas le dossier des noyaux de la cible [$ENV{MACOSX_DEPLOYMENT_TARGET}] (relire . scripts/mlx-sources.sh) / ${_sortie} is not the kernel directory of this target (source scripts/mlx-sources.sh again)")
  endif()

  # La preuve que lit la garde de mlx-sources.sh, et que seul ce fichier écrit :
  # un cache configuré sans lui, ou avec d'autres épingles, ne l'a pas.
  file(SHA256 "${CMAKE_CURRENT_LIST_DIR}/mlx-sources.sh" _epingles)
  file(SHA256 "${CMAKE_CURRENT_LIST_FILE}" _chaine)
  set(DYSFLOW_MLX_PREUVE "${_epingles} ${_chaine} ${_sortie}" CACHE INTERNAL "preuve de configuration")
endif()
