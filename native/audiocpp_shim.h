/*
 * Frontière C entre le lecteur Rust et audio.cpp.
 *
 * audio.cpp n'expose aucun `extern "C"` : son API est du C++ moderne
 * (std::unique_ptr, std::optional, exceptions). Ce shim la referme derrière une
 * façade C stable, avec deux règles absolues :
 *
 *   - aucune exception ne traverse la frontière : tout est capturé et converti
 *     en code de retour plus message ;
 *   - toute mémoire rendue à Rust est libérée par une fonction de ce fichier,
 *     jamais par l'allocateur de l'autre langage.
 *
 * Le découpage moteur/session reflète celui d'audio.cpp : le moteur détient les
 * poids (partagés, immuables), chaque session porte son propre graphe. Plusieurs
 * sessions peuvent donc synthétiser en parallèle sur un seul chargement.
 */
#ifndef RS_READER_AUDIOCPP_SHIM_H
#define RS_READER_AUDIOCPP_SHIM_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct acpp_engine acpp_engine;
typedef struct acpp_session acpp_session;

/* Audio rendu. `samples` appartient au shim jusqu'à `acpp_audio_free`. */
typedef struct {
    float * samples;
    size_t sample_count;
    int sample_rate;
    int channels;
} acpp_audio;

/* Conditionnement d'une requête. Les chaînes peuvent être NULL. */
typedef struct {
    const char * voice;    /* identifiant de voix, préréglage, ou chemin WAV */
    const char * language;
    const char * emotion;
    float speaking_rate;   /* <= 0 : ne pas transmettre */
    float pitch_shift;
    float energy;
    /* Options libres du modèle, transmises telles quelles. */
    const char * const * option_keys;
    const char * const * option_values;
    size_t option_count;
} acpp_params;

/*
 * Charge un modèle. Renvoie NULL en cas d'échec ; `out_error` reçoit alors un
 * message à libérer avec `acpp_string_free`.
 * `family` et `backend` peuvent être NULL ou vides (détection automatique).
 */
acpp_engine * acpp_engine_load(
    const char * model_path,
    const char * family,
    const char * backend,
    int threads,
    char ** out_error);

void acpp_engine_free(acpp_engine * engine);

/* Famille effectivement chargée. Chaîne à libérer avec `acpp_string_free`. */
char * acpp_engine_family(const acpp_engine * engine);

/* Vrai (1) si la famille ne sait parler qu'en clonant une voix de référence.
 *
 * Ces familles refusent toute requête sans audio de locuteur. Le savoir avant
 * la première phrase permet de le dire une fois, au lieu d'échouer autant de
 * fois qu'il y a de phrases. Renvoie -1 si le moteur n'est pas chargé. */
int acpp_engine_requires_reference(const acpp_engine * engine);

/* Ouvre une session de synthèse. Une session par thread. */
acpp_session * acpp_session_open(const acpp_engine * engine, char ** out_error);
void acpp_session_free(acpp_session * session);

/* 0 en cas de succès, -1 en cas d'échec. */
int acpp_session_synthesize(
    acpp_session * session,
    const char * text,
    const acpp_params * params,
    acpp_audio * out_audio,
    char ** out_error);

void acpp_audio_free(acpp_audio * audio);
void acpp_string_free(char * text);

#ifdef __cplusplus
}
#endif

#endif /* RS_READER_AUDIOCPP_SHIM_H */
