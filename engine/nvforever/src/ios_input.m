// [documented] Apple SDK GCKeyboardInput.h, GCMouseInput.h and UIViewController.h.
// Physical controls only; UIKit/winit still supplies text entry and touch events.
#import <GameController/GameController.h>
#import <UIKit/UIKit.h>
#import <objc/runtime.h>

typedef void (*InputSink)(unsigned kind, unsigned code, float x, float y);
static InputSink sink;
static BOOL capture;
static GCKeyboard *keyboard;
static GCMouse *mouse;

// Our wire codes, not Apple's key-code values. Keep in sync with ios_input.rs.
static unsigned key_code(GCKeyCode code) {
    GCKeyCode letters[] = {GCKeyCodeKeyA, GCKeyCodeKeyB, GCKeyCodeKeyC, GCKeyCodeKeyD,
        GCKeyCodeKeyE, GCKeyCodeKeyF, GCKeyCodeKeyG, GCKeyCodeKeyH, GCKeyCodeKeyI,
        GCKeyCodeKeyJ, GCKeyCodeKeyK, GCKeyCodeKeyL, GCKeyCodeKeyM, GCKeyCodeKeyN,
        GCKeyCodeKeyO, GCKeyCodeKeyP, GCKeyCodeKeyQ, GCKeyCodeKeyR, GCKeyCodeKeyS,
        GCKeyCodeKeyT, GCKeyCodeKeyU, GCKeyCodeKeyV, GCKeyCodeKeyW, GCKeyCodeKeyX,
        GCKeyCodeKeyY, GCKeyCodeKeyZ};
    for (unsigned i = 0; i < 26; ++i) if (code == letters[i]) return 'A' + i;
    GCKeyCode digits[] = {GCKeyCodeZero, GCKeyCodeOne, GCKeyCodeTwo, GCKeyCodeThree,
        GCKeyCodeFour, GCKeyCodeFive, GCKeyCodeSix, GCKeyCodeSeven, GCKeyCodeEight, GCKeyCodeNine};
    for (unsigned i = 0; i < 10; ++i) if (code == digits[i]) return '0' + i;
    GCKeyCode special[] = {GCKeyCodeEscape, GCKeyCodeTab, GCKeyCodeSpacebar,
        GCKeyCodeReturnOrEnter, GCKeyCodeDeleteOrBackspace, GCKeyCodeLeftShift,
        GCKeyCodeRightShift, GCKeyCodeLeftControl, GCKeyCodeRightControl,
        GCKeyCodeLeftAlt, GCKeyCodeRightAlt, GCKeyCodeCapsLock, GCKeyCodeGraveAccentAndTilde,
        GCKeyCodeUpArrow, GCKeyCodeDownArrow, GCKeyCodeLeftArrow, GCKeyCodeRightArrow,
        GCKeyCodeF1, GCKeyCodeF2, GCKeyCodeF3, GCKeyCodeF4, GCKeyCodeF5, GCKeyCodeF6,
        GCKeyCodeF7, GCKeyCodeF8, GCKeyCodeF9, GCKeyCodeF10, GCKeyCodeF11, GCKeyCodeF12,
        GCKeyCodeHyphen, GCKeyCodeEqualSign, GCKeyCodeOpenBracket, GCKeyCodeCloseBracket,
        GCKeyCodeBackslash, GCKeyCodeSemicolon, GCKeyCodeQuote, GCKeyCodeComma,
        GCKeyCodePeriod, GCKeyCodeSlash, GCKeyCodeHome, GCKeyCodeEnd,
        GCKeyCodePageUp, GCKeyCodePageDown, GCKeyCodeDeleteForward};
    for (unsigned i = 0; i < sizeof(special) / sizeof(special[0]); ++i)
        if (code == special[i]) return 256 + i;
    return 0;
}

static BOOL wants_lock(id object, SEL command) { return capture; }

@interface NVFPointerObserver : NSObject
- (void)hover:(UIHoverGestureRecognizer *)recognizer;
@end
@implementation NVFPointerObserver
- (void)hover:(UIHoverGestureRecognizer *)recognizer {
    if (!capture && recognizer.state != UIGestureRecognizerStateEnded) {
        CGPoint p = [recognizer locationInView:recognizer.view];
        sink(3, 0, p.x, p.y);
    }
}
@end

// Called only on Bevy's main thread. GC callbacks use the main queue too.
void nvf_ios_input_poll(InputSink callback, BOOL locked) {
    sink = callback;
    static id resign_observer;
    if (!resign_observer) {
        resign_observer = [NSNotificationCenter.defaultCenter addObserverForName:UIApplicationWillResignActiveNotification
            object:nil queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *note) { sink(4, 0, 0, 0); }];
    }
    GCKeyboard *next_keyboard = GCKeyboard.coalescedKeyboard;
    GCMouse *next_mouse = GCMouse.current ?: GCMouse.mice.firstObject;
    if (next_keyboard != keyboard || next_mouse != mouse) sink(4, 0, 0, 0);
    if (next_keyboard != keyboard) {
        keyboard.keyboardInput.keyChangedHandler = nil;
        keyboard = next_keyboard;
        keyboard.handlerQueue = dispatch_get_main_queue();
        keyboard.keyboardInput.keyChangedHandler = ^(GCKeyboardInput *input,
                GCDeviceButtonInput *button, GCKeyCode key, BOOL pressed) {
            unsigned code = key_code(key);
            if (code) sink(0, code, pressed, 0);
        };
        NSLog(@"NVForever physical keyboard: %@", keyboard ? @"connected" : @"disconnected");
    }
    if (next_mouse != mouse) {
        mouse.mouseInput.mouseMovedHandler = nil;
        mouse.mouseInput.leftButton.pressedChangedHandler = nil;
        mouse.mouseInput.rightButton.pressedChangedHandler = nil;
        mouse.mouseInput.middleButton.pressedChangedHandler = nil;
        mouse.mouseInput.scroll.valueChangedHandler = nil;
        mouse = next_mouse;
        mouse.handlerQueue = dispatch_get_main_queue();
        mouse.mouseInput.mouseMovedHandler = ^(GCMouseInput *input, float x, float y) {
            if (capture) sink(2, 0, x, -y);
        };
        mouse.mouseInput.leftButton.pressedChangedHandler = ^(GCControllerButtonInput *b, float v, BOOL pressed) { sink(1, 0, pressed, 0); };
        mouse.mouseInput.rightButton.pressedChangedHandler = ^(GCControllerButtonInput *b, float v, BOOL pressed) { sink(1, 1, pressed, 0); };
        mouse.mouseInput.middleButton.pressedChangedHandler = ^(GCControllerButtonInput *b, float v, BOOL pressed) { sink(1, 2, pressed, 0); };
        mouse.mouseInput.scroll.valueChangedHandler = ^(GCControllerDirectionPad *pad, float x, float y) { sink(5, 0, x, y); };
        NSLog(@"NVForever physical mouse: %@", mouse ? @"connected" : @"disconnected");
    }
    UIWindow *window = nil;
    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
        if (![scene isKindOfClass:UIWindowScene.class]) continue;
        for (UIWindow *candidate in ((UIWindowScene *)scene).windows)
            if (candidate.isKeyWindow) window = candidate;
    }
    UIViewController *controller = window.rootViewController;
    static __weak UIView *observed;
    static NVFPointerObserver *observer;
    if (controller && observed != controller.view) {
        observer = [NVFPointerObserver new];
        [controller.view addGestureRecognizer:[[UIHoverGestureRecognizer alloc] initWithTarget:observer action:@selector(hover:)]];
        // Add only to winit's application controller, never UIKit's base class.
        class_addMethod(controller.class, @selector(prefersPointerLocked), (IMP)wants_lock,
            method_getTypeEncoding(class_getInstanceMethod(UIViewController.class, @selector(prefersPointerLocked))));
        observed = controller.view;
    }
    BOOL wanted = locked && UIApplication.sharedApplication.applicationState == UIApplicationStateActive;
    if (capture != wanted) {
        capture = wanted;
        [controller setNeedsUpdateOfPrefersPointerLocked];
    }
}
