// Swift's do/catch does not see Objective-C exceptions: AVAudioEngine raises some (for example
// `com.apple.coreaudio.avfaudio` when AirPods switch between A2DP and HFP while the graph is
// being built), and an uncaught one aborts the sidecar. This wrapper turns them into NSError.

#import "StenoAudioRT.h"

NSErrorDomain const StenoObjCExceptionDomain = @"StenoObjCExceptionDomain";

@implementation StenoObjCExceptionCatcher

+ (BOOL)catchingBlock:(NS_NOESCAPE void (^)(void))block error:(NSError *_Nullable *_Nullable)error {
    @try {
        block();
        return YES;
    } @catch (NSException *exception) {
        if (error) {
            NSString *description = [NSString stringWithFormat:@"%@: %@", exception.name,
                                                               exception.reason ?: @"(no reason)"];
            NSMutableDictionary *info = [NSMutableDictionary dictionary];
            info[NSLocalizedDescriptionKey] = description;
            info[@"ExceptionName"] = exception.name;
            if (exception.reason) info[@"ExceptionReason"] = exception.reason;
            *error = [NSError errorWithDomain:StenoObjCExceptionDomain code:1 userInfo:info];
        }
        return NO;
    }
}

@end
