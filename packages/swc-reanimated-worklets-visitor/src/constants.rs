use std::collections::HashMap;

use once_cell::sync::Lazy;

pub static OBJECT_HOOKS: [&str; 2] = ["useAnimatedGestureHandler", "useAnimatedScrollHandler"];

pub static POSSIBLE_OPT_FUNCTION: [&str; 1] = ["interpolate"];

pub static GESTURE_HANDLER_BUILDER_METHODS: [&str; 10] = [
    "onBegin",
    "onStart",
    "onEnd",
    "onFinalize",
    "onUpdate",
    "onChange",
    "onTouchesDown",
    "onTouchesMove",
    "onTouchesUp",
    "onTouchesCancelled",
];

pub static GESTURE_HANDLER_GESTURE_OBJECTS: [&str; 12] = [
    // from https://github.com/software-mansion/react-native-gesture-handler/blob/new-api/src/handlers/gestures/gestureObjects.ts
    "Tap",
    "Pan",
    "Pinch",
    "Rotation",
    "Fling",
    "LongPress",
    "ForceTouch",
    "Native",
    "Manual",
    "Race",
    "Simultaneous",
    "Exclusive",
];

pub static GLOBALS: [&str; 54] = [
    "this",
    "console",
    "performance",
    "_setGlobalConsole",
    "_chronoNow",
    "Date",
    "Array",
    "ArrayBuffer",
    "Int8Array",
    "Int16Array",
    "Int32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Uint16Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "Date",
    "HermesInternal",
    "JSON",
    "Math",
    "Number",
    "Object",
    "String",
    "Symbol",
    "undefined",
    "null",
    "UIManager",
    "requestAnimationFrame",
    "_WORKLET",
    "arguments",
    "Boolean",
    "parseInt",
    "parseFloat",
    "Map",
    "Set",
    "_log",
    "_updatePropsPaper",
    "_updatePropsFabric",
    "_removeShadowNodeFromRegistry",
    "RegExp",
    "Error",
    "global",
    "_measure",
    "_scrollTo",
    "_dispatchCommand",
    "_setGestureState",
    "_getCurrentTime",
    "_eventTimestamp",
    "_frameTimestamp",
    "isNaN",
    "LayoutAnimationRepository",
    "_stopObservingProgress",
    "_startObservingProgress",
];

pub static FUNCTION_ARGS_TO_WORKLETIZE: Lazy<HashMap<&'static str, Vec<usize>>> = Lazy::new(|| {
    HashMap::from([
        ("useAnimatedStyle", vec![0]),
        ("useAnimatedProps", vec![0]),
        ("createAnimatedPropAdapter", vec![0]),
        ("useDerivedValue", vec![0]),
        ("useAnimatedScrollHandler", vec![0]),
        ("useAnimatedReaction", vec![0, 1]),
        ("useWorkletCallback", vec![0]),
        ("createWorklet", vec![0]),
        // animations' callbacks
        ("withTiming", vec![2]),
        ("withSpring", vec![2]),
        ("withDecay", vec![1]),
        ("withRepeat", vec![3]),
    ])
});

pub static FUNCTIONLESS_FLAG: i32 = 0b00000001;
pub static STATEMENTLESS_FLAG: i32 = 0b00000010;
