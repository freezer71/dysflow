/* Compile Sonic (third_party/sonic, non modifié) en comptant ses appels au tas
 * (calloc, realloc, free).
 *
 * Les en-têtes système sont inclus d'abord : leurs gardes d'inclusion rendent
 * inopérants ceux de sonic.c, si bien que les macros ci-dessous ne renomment
 * que les appels de Sonic, pas les déclarations de la libc.
 *
 * Le compte, propre à chaque fil, permet de vérifier qu'aucune allocation ne
 * se produit dans le fil audio une fois la phrase préparée. */
#include <limits.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>

static _Thread_local unsigned long long sillon_sonic_heap_calls = 0;

unsigned long long sillon_sonic_heap_calls_on_this_thread(void) {
    return sillon_sonic_heap_calls;
}

static void *sillon_sonic_calloc(size_t num, size_t size) {
    sillon_sonic_heap_calls++;
    return calloc(num, size);
}

static void *sillon_sonic_realloc(void *p, size_t size) {
    sillon_sonic_heap_calls++;
    return realloc(p, size);
}

static void sillon_sonic_free(void *p) {
    if (p) {
        sillon_sonic_heap_calls++;
    }
    free(p);
}

#define calloc sillon_sonic_calloc
#define realloc sillon_sonic_realloc
#define free sillon_sonic_free
#include "sonic.c"
