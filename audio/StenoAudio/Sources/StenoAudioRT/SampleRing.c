// Lock-free single-producer / single-consumer ring of Float32 samples.
//
// `head` (written by the producer) and `tail` (written by the consumer) are free-running 32-bit
// counters; the capacity is a power of two, so `head - tail` is the fill level even after they
// wrap around. Release stores publish the samples, acquire loads observe them.

#include "StenoAudioRT.h"

#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>

struct steno_ring {
    float *data;
    uint32_t capacity;
    uint32_t mask;
    _Atomic uint32_t head;
    _Atomic uint32_t tail;
    _Atomic uint64_t dropped;
};

steno_ring *steno_ring_create(uint32_t min_capacity) {
    uint32_t capacity = 1024;
    while (capacity < min_capacity && capacity < (1u << 30)) capacity <<= 1;
    steno_ring *ring = calloc(1, sizeof(steno_ring));
    if (!ring) return NULL;
    ring->data = calloc(capacity, sizeof(float));
    if (!ring->data) {
        free(ring);
        return NULL;
    }
    ring->capacity = capacity;
    ring->mask = capacity - 1;
    atomic_init(&ring->head, 0);
    atomic_init(&ring->tail, 0);
    atomic_init(&ring->dropped, 0);
    return ring;
}

void steno_ring_destroy(steno_ring *ring) {
    free(ring->data);
    free(ring);
}

static inline uint32_t ring_free_space(steno_ring *ring, uint32_t head) {
    uint32_t tail = atomic_load_explicit(&ring->tail, memory_order_acquire);
    return ring->capacity - (head - tail);
}

uint32_t steno_ring_write(steno_ring *ring, const float *samples, uint32_t count) {
    uint32_t head = atomic_load_explicit(&ring->head, memory_order_relaxed);
    uint32_t space = ring_free_space(ring, head);
    uint32_t n = count < space ? count : space;
    for (uint32_t i = 0; i < n; i++) ring->data[(head + i) & ring->mask] = samples[i];
    atomic_store_explicit(&ring->head, head + n, memory_order_release);
    if (n < count) atomic_fetch_add_explicit(&ring->dropped, count - n, memory_order_relaxed);
    return n;
}

static inline uint32_t buffer_frames(const AudioBuffer *buf) {
    if (!buf->mData || buf->mNumberChannels == 0) return 0;
    return buf->mDataByteSize / (uint32_t)sizeof(float) / buf->mNumberChannels;
}

uint32_t steno_ring_write_mono(steno_ring *ring, const AudioBufferList *list,
                               int32_t first_buffer, uint32_t buffer_count) {
    if (!list || list->mNumberBuffers == 0 || buffer_count == 0) return 0;
    int64_t first = first_buffer >= 0 ? first_buffer : (int64_t)list->mNumberBuffers + first_buffer;
    if (first < 0) first = 0;
    if (first >= list->mNumberBuffers) return 0;
    uint32_t end = (uint32_t)first + buffer_count;
    if (end > list->mNumberBuffers) end = list->mNumberBuffers;

    // Frames: the shortest of the selected buffers; channels: all of theirs.
    uint32_t frames = UINT32_MAX;
    uint32_t total_channels = 0;
    for (uint32_t b = (uint32_t)first; b < end; b++) {
        uint32_t n = buffer_frames(&list->mBuffers[b]);
        if (n == 0) continue;
        if (n < frames) frames = n;
        total_channels += list->mBuffers[b].mNumberChannels;
    }
    if (total_channels == 0 || frames == UINT32_MAX) return 0;
    float scale = 1.0f / (float)total_channels;

    uint32_t head = atomic_load_explicit(&ring->head, memory_order_relaxed);
    uint32_t space = ring_free_space(ring, head);
    uint32_t n = frames < space ? frames : space;
    for (uint32_t f = 0; f < n; f++) {
        float sum = 0;
        for (uint32_t b = (uint32_t)first; b < end; b++) {
            const AudioBuffer *buf = &list->mBuffers[b];
            if (buffer_frames(buf) == 0) continue;
            const float *p = (const float *)buf->mData + (size_t)f * buf->mNumberChannels;
            for (uint32_t c = 0; c < buf->mNumberChannels; c++) sum += p[c];
        }
        ring->data[(head + f) & ring->mask] = sum * scale;
    }
    atomic_store_explicit(&ring->head, head + n, memory_order_release);
    if (n < frames) atomic_fetch_add_explicit(&ring->dropped, frames - n, memory_order_relaxed);
    return n;
}

uint32_t steno_ring_available(steno_ring *ring) {
    uint32_t head = atomic_load_explicit(&ring->head, memory_order_acquire);
    uint32_t tail = atomic_load_explicit(&ring->tail, memory_order_relaxed);
    return head - tail;
}

uint32_t steno_ring_read(steno_ring *ring, float *dst, uint32_t max) {
    uint32_t head = atomic_load_explicit(&ring->head, memory_order_acquire);
    uint32_t tail = atomic_load_explicit(&ring->tail, memory_order_relaxed);
    uint32_t available = head - tail;
    uint32_t n = available < max ? available : max;
    for (uint32_t i = 0; i < n; i++) dst[i] = ring->data[(tail + i) & ring->mask];
    atomic_store_explicit(&ring->tail, tail + n, memory_order_release);
    return n;
}

uint64_t steno_ring_take_dropped(steno_ring *ring) {
    return atomic_exchange_explicit(&ring->dropped, 0, memory_order_relaxed);
}
