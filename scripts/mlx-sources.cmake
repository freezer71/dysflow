# Fichier de chaîne d'outils (toolchain) de la copie Rust de MLX : il range les
# cinq sources que mlx-sys (mlx-c, CMake FetchContent) irait chercher sur le
# réseau, et refuse toute autre. Les sources viennent de scripts/mlx-sources.sh
# (commits et empreintes épinglés) ; ce fichier ne télécharge rien.
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
