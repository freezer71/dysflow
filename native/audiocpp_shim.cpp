#include "audiocpp_shim.h"

#include "engine/framework/audio/conversion.h"
#include "engine/framework/audio/wav_reader.h"
#include "engine/framework/core/backend.h"
#include "engine/framework/core/module.h"
#include "engine/framework/runtime/model.h"
#include "engine/framework/runtime/registry.h"
#include "engine/framework/runtime/session.h"

#include <cstring>
#include <exception>
#include <filesystem>
#include <memory>
#include <new>
#include <string>
#include <vector>

namespace {

namespace rt = engine::runtime;

/* Copie une chaîne C++ vers une allocation que Rust rendra via
 * acpp_string_free. `strdup` n'est pas garanti par le standard C++. */
char * duplicate(const std::string & value) {
    auto * copy = static_cast<char *>(std::malloc(value.size() + 1));
    if (copy == nullptr) {
        return nullptr;
    }
    std::memcpy(copy, value.c_str(), value.size() + 1);
    return copy;
}

void report(char ** out_error, const std::string & message) {
    if (out_error != nullptr) {
        *out_error = duplicate(message);
    }
}

std::string text_or_empty(const char * value) {
    return value == nullptr ? std::string() : std::string(value);
}

engine::core::BackendType parse_backend(const std::string & name) {
    // Les noms suivent ceux de la ligne de commande d'audio.cpp.
    if (name == "cuda") {
        return engine::core::BackendType::Cuda;
    }
    if (name == "metal") {
        return engine::core::BackendType::Metal;
    }
    if (name == "vulkan") {
        return engine::core::BackendType::Vulkan;
    }
    if (name == "hip" || name == "rocm") {
        return engine::core::BackendType::Hip;
    }
    if (name == "auto" || name == "best") {
        return engine::core::BackendType::BestAvailable;
    }
    return engine::core::BackendType::Cpu;
}

/* Une voix est soit un fichier WAV de référence, soit un identifiant connu du
 * modèle. On distingue les deux par l'existence du chemin. */
bool looks_like_path(const std::string & voice) {
    if (voice.empty()) {
        return false;
    }
    std::error_code error;
    return std::filesystem::is_regular_file(std::filesystem::path(voice), error);
}

}  // namespace

struct acpp_engine {
    rt::ModelRegistry registry;
    std::unique_ptr<rt::ILoadedVoiceModel> model;
    engine::core::BackendConfig backend;
};

struct acpp_session {
    std::unique_ptr<rt::IVoiceTaskSession> session;
    /* Vue non possédante sur la même session, pour l'exécution hors-ligne. */
    rt::IOfflineVoiceTaskSession * offline = nullptr;
};

acpp_engine * acpp_engine_load(
    const char * model_path,
    const char * family,
    const char * backend,
    int threads,
    char ** out_error) {
    try {
        const std::string path = text_or_empty(model_path);
        if (path.empty()) {
            report(out_error, "aucun modèle configuré (engine.model_path)");
            return nullptr;
        }
        if (!std::filesystem::exists(path)) {
            report(out_error, "modèle introuvable : " + path);
            return nullptr;
        }

        auto engine = std::make_unique<acpp_engine>();
        engine->registry = rt::make_default_registry();

        rt::ModelLoadRequest request;
        request.model_path = std::filesystem::path(path);
        const std::string family_hint = text_or_empty(family);
        if (!family_hint.empty()) {
            request.family_hint = family_hint;
        }

        engine->model = engine->registry.load(request);
        if (!engine->model) {
            report(out_error, "le registre n'a produit aucun modèle");
            return nullptr;
        }

        engine->backend.type = parse_backend(text_or_empty(backend));
        engine->backend.threads = threads > 0 ? threads : 1;
        engine->backend.device = 0;

        return engine.release();
    } catch (const std::exception & error) {
        report(out_error, error.what());
        return nullptr;
    } catch (...) {
        report(out_error, "erreur inconnue au chargement du modèle");
        return nullptr;
    }
}

void acpp_engine_free(acpp_engine * engine) {
    delete engine;
}

char * acpp_engine_family(const acpp_engine * engine) {
    try {
        if (engine == nullptr || !engine->model) {
            return duplicate("inconnu");
        }
        return duplicate(engine->model->metadata().family);
    } catch (...) {
        return duplicate("inconnu");
    }
}

int acpp_engine_requires_reference(const acpp_engine * engine) {
    try {
        if (engine == nullptr || !engine->model) {
            return -1;
        }
        const rt::CapabilitySet & caps = engine->model->capabilities();
        bool tts = false;
        bool cloning = false;
        for (const auto & entry : caps.supported_tasks) {
            for (const auto mode : entry.modes) {
                if (mode != rt::RunMode::Offline) {
                    continue;
                }
                if (entry.task == rt::VoiceTaskKind::Tts) {
                    tts = true;
                } else if (entry.task == rt::VoiceTaskKind::VoiceCloning) {
                    cloning = true;
                }
            }
        }
        return (!tts && cloning) ? 1 : 0;
    } catch (...) {
        return -1;
    }
}

acpp_session * acpp_session_open(const acpp_engine * engine, char ** out_error) {
    try {
        if (engine == nullptr || !engine->model) {
            report(out_error, "moteur non chargé");
            return nullptr;
        }

        // La tâche est choisie d'après ce que le modèle annonce, et non fixée à
        // « synthèse ». Plusieurs familles — Chatterbox la première — ne savent
        // parler qu'en clonant : elles n'exposent que `VoiceCloning`, et une
        // demande de `Tts` se solde par un refus sec (« Chatterbox session
        // supports --task clon or --task vc »).
        const rt::CapabilitySet & caps = engine->model->capabilities();
        const auto supports = [&caps](rt::VoiceTaskKind kind) {
            for (const auto & entry : caps.supported_tasks) {
                if (entry.task != kind) {
                    continue;
                }
                for (const auto mode : entry.modes) {
                    if (mode == rt::RunMode::Offline) {
                        return true;
                    }
                }
            }
            return false;
        };

        rt::TaskSpec task;
        task.mode = rt::RunMode::Offline;
        if (supports(rt::VoiceTaskKind::Tts)) {
            task.task = rt::VoiceTaskKind::Tts;
        } else if (supports(rt::VoiceTaskKind::VoiceCloning)) {
            task.task = rt::VoiceTaskKind::VoiceCloning;
        } else {
            report(out_error, "cette famille ne sait ni synthetiser ni cloner hors-ligne");
            return nullptr;
        }

        rt::SessionOptions options;
        options.backend = engine->backend;

        auto created = engine->model->create_task_session(task, options);
        if (!created) {
            report(out_error, "la famille n'a pas ouvert de session");
            return nullptr;
        }

        // La session doit savoir exécuter une requête hors-ligne ; certaines
        // familles n'exposent que le mode flux.
        auto * offline = dynamic_cast<rt::IOfflineVoiceTaskSession *>(created.get());
        if (offline == nullptr) {
            report(out_error, "cette famille n'expose pas de synthèse hors-ligne");
            return nullptr;
        }

        auto session = std::make_unique<acpp_session>();
        session->session = std::move(created);
        session->offline = offline;
        return session.release();
    } catch (const std::exception & error) {
        report(out_error, error.what());
        return nullptr;
    } catch (...) {
        report(out_error, "erreur inconnue à l'ouverture de session");
        return nullptr;
    }
}

void acpp_session_free(acpp_session * session) {
    delete session;
}

int acpp_session_synthesize(
    acpp_session * session,
    const char * text,
    const acpp_params * params,
    acpp_audio * out_audio,
    char ** out_error) {
    try {
        if (session == nullptr || session->offline == nullptr) {
            report(out_error, "session invalide");
            return -1;
        }
        if (out_audio == nullptr) {
            report(out_error, "tampon de sortie manquant");
            return -1;
        }
        const std::string content = text_or_empty(text);
        if (content.empty()) {
            report(out_error, "texte vide");
            return -1;
        }

        rt::TaskRequest request;
        request.text_input = rt::Transcript{
            content,
            params != nullptr ? text_or_empty(params->language) : std::string(),
        };

        if (params != nullptr) {
            for (size_t i = 0; i < params->option_count; ++i) {
                if (params->option_keys[i] == nullptr || params->option_values[i] == nullptr) {
                    continue;
                }
                request.options[params->option_keys[i]] = params->option_values[i];
            }

            // Le style porte l'intonation, le débit et la hauteur. Appliquer le
            // débit ici, et non par ré-échantillonnage après coup, évite de
            // transposer la voix.
            rt::StyleCondition style;
            bool has_style = false;
            const std::string language = text_or_empty(params->language);
            if (!language.empty()) {
                style.language = language;
                has_style = true;
            }
            const std::string emotion = text_or_empty(params->emotion);
            if (!emotion.empty()) {
                style.emotion = emotion;
                has_style = true;
            }
            if (params->speaking_rate > 0.0f) {
                style.speaking_rate = params->speaking_rate;
                has_style = true;
            }
            if (params->pitch_shift != 0.0f) {
                style.pitch_shift = params->pitch_shift;
                has_style = true;
            }
            if (params->energy > 0.0f) {
                style.energy_scale = params->energy;
                has_style = true;
            }

            rt::VoiceCondition voice;
            bool has_voice = false;
            const std::string voice_name = text_or_empty(params->voice);
            if (!voice_name.empty()) {
                rt::VoiceReference speaker;
                if (looks_like_path(voice_name)) {
                    // Clonage : le WAV devient la référence de locuteur.
                    //
                    // Il est ramené en mono avant d'être transmis. Chatterbox
                    // refuse le stéréo (« S3 prompt mel expects mono audio »),
                    // et c'est précisément ce que produit un enregistrement de
                    // téléphone ou de dictaphone : la référence la plus
                    // naturelle à fournir serait aussi la seule à échouer.
                    const auto wav = engine::audio::read_wav_f32(
                        std::filesystem::path(voice_name));
                    rt::AudioBuffer buffer;
                    buffer.sample_rate = wav.sample_rate;
                    buffer.channels = 1;
                    buffer.samples = engine::audio::convert_wav_to_mono_linear_resampled(
                        wav, wav.sample_rate);
                    speaker.audio = std::move(buffer);
                } else {
                    speaker.cached_voice_id = voice_name;
                }
                voice.speaker = std::move(speaker);
                has_voice = true;
            }
            if (has_style) {
                voice.style = std::move(style);
                has_voice = true;
            }
            if (has_voice) {
                request.voice = std::move(voice);
            }
        }

        // Le contrat d'audio.cpp impose de préparer la session avec la requête
        // avant de l'exécuter : sans cela, les familles refusent de tourner
        // (« session prepare() must be called before … run »).
        session->session->prepare(rt::build_preparation_request(request));

        const rt::TaskResult result = session->offline->run(request);
        if (!result.audio_output.has_value()) {
            report(out_error, "le modèle n'a produit aucun audio");
            return -1;
        }

        const rt::AudioBuffer & audio = *result.audio_output;
        if (audio.samples.empty()) {
            report(out_error, "audio vide");
            return -1;
        }

        const size_t bytes = audio.samples.size() * sizeof(float);
        auto * samples = static_cast<float *>(std::malloc(bytes));
        if (samples == nullptr) {
            report(out_error, "allocation impossible");
            return -1;
        }
        std::memcpy(samples, audio.samples.data(), bytes);

        out_audio->samples = samples;
        out_audio->sample_count = audio.samples.size();
        out_audio->sample_rate = audio.sample_rate;
        out_audio->channels = audio.channels > 0 ? audio.channels : 1;
        return 0;
    } catch (const std::exception & error) {
        report(out_error, error.what());
        return -1;
    } catch (...) {
        report(out_error, "erreur inconnue pendant la synthèse");
        return -1;
    }
}

void acpp_audio_free(acpp_audio * audio) {
    if (audio == nullptr) {
        return;
    }
    std::free(audio->samples);
    audio->samples = nullptr;
    audio->sample_count = 0;
}

void acpp_string_free(char * text) {
    std::free(text);
}
