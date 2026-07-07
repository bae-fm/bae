#import "BaeObjCExceptionGuard.h"

@implementation BaeObjCExceptionGuard

+ (nullable id)valueForKey:(NSString *)key onObject:(id)object {
    @try {
        return [object valueForKey:key];
    } @catch (NSException *exception) {
        return nil;
    }
}

@end
