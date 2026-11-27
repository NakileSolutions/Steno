// Small C / Objective-C helpers the Swift code cannot express safely:
// - a lock-free single-producer / single-consumer sample ring, written from the Core Audio
//   IO thread (no allocation, no lock, no syscall);
// - a wrapper that turns Objective-C exceptions (raised by AVAudioEngine) into NSError.

#ifndef STENO_AUDIO_RT_H
#define STENO_AUDIO_RT_H

#include <stdbool.h>
#include <stdint.h>
#include <CoreAudio/CoreAudioTypes.h>
#ifdef __OBJC__
#import <Foundation/Foundation.h>
#endif

#pragma clang assume_nonnull begin

// MARK: - Sample ring (SPSC)

typedef struct steno_ring steno_ring;

/// Creates a ring holding at least `min_capacity` Float32 samples (rounded up to a power of two).
/// Returns NULL if the allocation fails. Not real-time safe: call it before starting the IOProc.
steno_ring *_Nullable steno_ring_create(uint32_t min_capacity);

/// Frees the ring. The IOProc writing to it must be stopped first.
void steno_ring_destroy(steno_ring *ring);

/// Producer side (real-time safe): averages every channel of `buffer_count` Float32 buffers of
/// `list`, starting at `first_buffer` (negative: counted from the end, -2 = the last two), and
/// appends the mono result. Works with interleaved (one buffer, N channels) and non-interleaved
/// (one buffer per channel) layouts. Samples that do not fit are dropped and counted. Returns the
/// number of frames written.
uint32_t steno_ring_write_mono(steno_ring *ring, const AudioBufferList *list,
                               int32_t first_buffer, uint32_t buffer_count);

/// Producer side (real-time safe): appends `count` mono samples.
uint32_t steno_ring_write(steno_ring *ring, const float *samples, uint32_t count);

/// Consumer side: number of samples ready to be read.
uint32_t steno_ring_available(steno_ring *ring);

/// Consumer side: copies up to `max` samples into `dst`; returns the number copied.
uint32_t steno_ring_read(steno_ring *ring, float *dst, uint32_t max);

/// Consumer side: frames dropped because the ring was full since the previous call.
uint64_t steno_ring_take_dropped(steno_ring *ring);

// MARK: - Objective-C exceptions

#ifdef __OBJC__

/// Runs `block` and converts an Objective-C exception it raises into an NSError (domain
/// `StenoObjCExceptionDomain`, the exception's name and reason in the description).
@interface StenoObjCExceptionCatcher : NSObject
+ (BOOL)catchingBlock:(NS_NOESCAPE void (^)(void))block error:(NSError *_Nullable *_Nullable)error
    NS_SWIFT_NAME(catching(_:));
@end

extern NSErrorDomain const StenoObjCExceptionDomain;
#endif

#pragma clang assume_nonnull end

#endif
