/*
 * dysflow-espeak : la phonétisation d'espeak-ng, derrière un protocole binaire.
 *
 * Copyright (C) 2026 Tharsan Vishnukumar
 *
 * This program is free software: you can redistribute it and/or modify it
 * under the terms of the GNU General Public License as published by the Free
 * Software Foundation, either version 3 of the License, or (at your option)
 * any later version. It is distributed WITHOUT ANY WARRANTY; see the GNU
 * General Public License (COPYING, shipped alongside) for more details.
 *
 * Programme séparé, lié à libespeak-ng (GPL-3.0) : Dysflow ne s'y lie pas, il
 * le lance et lui parle par ses entrées et sorties standard.
 *
 *   dysflow-espeak [DOSSIER]   (DOSSIER contient espeak-ng-data)
 *
 * Protocole, entiers u32 petit-boutistes ; une trame = u32 longueur + octets.
 *   - au lancement : une trame, le nom de la voix ; réponse : la trame « ok ».
 *   - puis, pour chaque lot : u32 nombre d'entrées, puis une trame UTF-8 par
 *     entrée ; réponse : une trame de phonèmes IPA par entrée, dans l'ordre,
 *     puis la trame « ok ».
 * Le découpage ne dépend que des longueurs : aucun texte ne peut le déplacer.
 * Toute erreur (voix inconnue, trame trop longue, entrée coupée) termine le
 * programme ; Dysflow le relance.
 *
 * Sous Windows, DOSSIER est lu en UTF-16 (wmain) et le programme s'y place :
 * libespeak-ng, qui ouvre ses fichiers par des chemins en octets (page de
 * code locale), n'en voit que « . ». DOSSIER peut ainsi contenir n'importe
 * quel caractère (« C:\Users\Łukasz… »). Sa longueur reste bornée : sans
 * longPathAware dans le manifeste, le dossier courant d'un programme Windows
 * ne dépasse pas MAX_PATH (260 caractères, ~258 utiles).
 *
 * Les phonèmes viennent d'espeak_TextToPhonemes, comme chez phonemizer (que
 * misaki utilise) : en IPA, les phonèmes de plusieurs lettres liés par U+0361,
 * une proposition après l'autre, réunies par une espace. Ni SSML ni phonèmes
 * entre [[ ]] : rien n'appelle espeak_Synth, qui seul les active.
 */

#include <espeak-ng/speak_lib.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
/* Windows (MinGW-w64) : les mêmes appels sous leurs noms du CRT, et NUL pour
 * /dev/null. Les entrées et sorties passent en binaire : en mode texte, le
 * CRT changerait les \n en \r\n et s'arrêterait au premier octet 0x1A. */
#include <direct.h>
#include <io.h>
#include <wchar.h>
#define dup _dup
#define dup2 _dup2
#define close _close
#define open _open
#define fdopen _fdopen
#define O_WRONLY _O_WRONLY
#define STDOUT_FILENO 1
#define NULL_DEVICE "NUL"
#else
#include <unistd.h>
#define NULL_DEVICE "/dev/null"
#endif

#define MAX_ITEM (64 * 1024)
#define MAX_COUNT 4096
#define MAX_CLAUSES 100000
/* Bit 1 : IPA ; bit 7 : bits 8-23 (U+0361) comme lien. */
#define MODE (0x02 | 0x80 | (0x0361 << 8))

/* Les trames sortent par un double privé de la sortie standard : libespeak-ng
 * peut écrire sur fd 1 (« Invalid phoneme code »), qui pointe sur /dev/null,
 * et rien ne se glisse dans le protocole. */
static FILE *out;

static int read_full(void *buffer, size_t size) {
	return fread(buffer, 1, size, stdin) == size;
}

static int read_u32(uint32_t *value) {
	unsigned char b[4];
	if (!read_full(b, 4))
		return 0;
	*value = (uint32_t)b[0] | (uint32_t)b[1] << 8 | (uint32_t)b[2] << 16 | (uint32_t)b[3] << 24;
	return 1;
}

static void write_u32(uint32_t value) {
	unsigned char b[4];
	b[0] = value & 0xff;
	b[1] = (value >> 8) & 0xff;
	b[2] = (value >> 16) & 0xff;
	b[3] = value >> 24;
	fwrite(b, 1, 4, out);
}

static void write_frame(const char *bytes, size_t size) {
	write_u32((uint32_t)size);
	fwrite(bytes, 1, size, out);
}

/* Une trame lue dans un tampon terminé par un zéro ; NULL si trop longue. */
static char *read_frame(void) {
	uint32_t size;
	if (!read_u32(&size) || size > MAX_ITEM)
		return NULL;
	char *text = malloc(size + 1);
	if (text == NULL || !read_full(text, size)) {
		free(text);
		return NULL;
	}
	text[size] = 0;
	return text;
}

static char *output;
static size_t output_size, output_capacity;

static void append(const char *bytes, size_t size) {
	if (output_size + size + 1 > output_capacity) {
		output_capacity = (output_size + size + 1) * 2;
		output = realloc(output, output_capacity);
		if (output == NULL)
			exit(4);
	}
	memcpy(output + output_size, bytes, size);
	output_size += size;
}

static void phonemize(const char *text) {
	output_size = 0;
	const void *cursor = text;
	for (int clause = 0; cursor != NULL && clause < MAX_CLAUSES; clause++) {
		const char *phonemes = espeak_TextToPhonemes(&cursor, espeakCHARS_UTF8, MODE);
		if (phonemes == NULL)
			break;
		if (*phonemes == 0)
			continue;
		if (output_size > 0)
			append(" ", 1);
		append(phonemes, strlen(phonemes));
	}
	write_frame(output, output_size);
}

static int run(const char *path) {
	int private_fd = dup(STDOUT_FILENO);
	int null_fd = open(NULL_DEVICE, O_WRONLY);
	if (private_fd < 0 || null_fd < 0 || dup2(null_fd, STDOUT_FILENO) < 0)
		return 2;
	close(null_fd);
#ifdef _WIN32
	if (_setmode(_fileno(stdin), _O_BINARY) < 0 || _setmode(private_fd, _O_BINARY) < 0)
		return 2;
#endif
	out = fdopen(private_fd, "wb");
	if (out == NULL)
		return 2;
	if (espeak_Initialize(AUDIO_OUTPUT_SYNCHRONOUS, 0, path, espeakINITIALIZE_DONT_EXIT) < 0)
		return 2;
	char *voice = read_frame();
	if (voice == NULL)
		return 2;
	/* Comme `espeak-ng -v` : par nom, sinon par langue (« fr-fr », « en-us »). */
	if (espeak_SetVoiceByName(voice) != EE_OK) {
		espeak_VOICE selector;
		memset(&selector, 0, sizeof(selector));
		selector.languages = voice;
		if (espeak_SetVoiceByProperties(&selector) != EE_OK)
			return 2;
	}
	free(voice);
	write_frame("ok", 2);
	fflush(out);
	uint32_t count;
	while (read_u32(&count)) {
		if (count > MAX_COUNT)
			return 3;
		for (uint32_t i = 0; i < count; i++) {
			char *text = read_frame();
			if (text == NULL)
				return 3;
			phonemize(text);
			free(text);
		}
		write_frame("ok", 2);
		fflush(out);
	}
	return 0;
}

#ifdef _WIN32
int wmain(int argc, wchar_t **argv) {
	if (argc > 1 && _wchdir(argv[1]) != 0)
		return 2;
	return run(argc > 1 ? "." : NULL);
}
#else
int main(int argc, char **argv) {
	return run(argc > 1 ? argv[1] : NULL);
}
#endif
