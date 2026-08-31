# Construit le shim C au-dessus d'audio.cpp.
#
# Le shim est une bibliothèque partagée : CMake résout ainsi lui-même toutes les
# dépendances transitives (ggml et ses backends, OpenMP, la bibliothèque
# standard C++), là où un lien statique obligerait `build.rs` à reconstituer un
# ordre d'édition de liens fragile.
cmake_minimum_required(VERSION 3.20)
project(rs_reader_audiocpp_shim CXX)

if (NOT DEFINED AUDIOCPP_DIR)
    message(FATAL_ERROR "AUDIOCPP_DIR doit pointer sur l'arbre source d'audio.cpp")
endif()

set(CMAKE_POSITION_INDEPENDENT_CODE ON)

# EXCLUDE_FROM_ALL : on ne veut que les bibliothèques, pas leurs exécutables.
add_subdirectory(${AUDIOCPP_DIR} audiocpp EXCLUDE_FROM_ALL)

add_library(audiocpp_shim SHARED audiocpp_shim.cpp)

target_compile_features(audiocpp_shim PRIVATE cxx_std_20)
target_include_directories(audiocpp_shim
    PRIVATE
        ${AUDIOCPP_DIR}/include
        ${CMAKE_CURRENT_SOURCE_DIR}
)

# La même ligne que celle d'audiocpp_cli : engine_runtime agrège déjà les
# objets du framework et des familles de modèles retenues.
target_link_libraries(audiocpp_shim PRIVATE engine_runtime ggml)

if (ENGINE_ENABLE_OPENMP AND TARGET OpenMP::OpenMP_CXX)
    target_link_libraries(audiocpp_shim PRIVATE OpenMP::OpenMP_CXX)
endif()

install(TARGETS audiocpp_shim
    LIBRARY DESTINATION lib
    ARCHIVE DESTINATION lib
    RUNTIME DESTINATION bin
)
