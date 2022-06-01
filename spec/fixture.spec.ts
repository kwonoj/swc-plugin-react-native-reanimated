import * as path from "path";

const options = {
  filename: path.join(path.resolve(__dirname, ".."), "jest tests fixture"),
  jsc: {
    parser: {
      syntax: "ecmascript",
      jsx: true,
    },
    target: "es2022",
    preserveAllComments: true,
    experimental: {},
  },
  isModule: true,
  module: {
    type: "commonjs",
  },
};

const transformPresets: Array<
  [
    string,
    (code: string) => ReturnType<typeof import("@swc/core").transformSync>
  ]
> = [
    [
      "plugin",
      (code: string) => {
        const opt = { ...options };
        opt.jsc.experimental = {
          plugins: [
            [
              path.resolve(
                __dirname,
                "../target/wasm32-wasi/debug/swc_plugin_reanimated.wasm"
              ),
              {},
            ],
          ],
        };

        const { transformSync } = require("@swc/core");
        return transformSync(code, opt);
      },
    ],
    [
      "custom transform",
      (code: string) => {
        const { transformSync } = require("../index");
        return transformSync(code, true, Buffer.from(JSON.stringify(options)));
      },
    ]
    ,
  ];

describe.each(transformPresets)("fixture with %s", (_, executeTransform) => {
  it.skip("transforms", () => {
    const input = `
    import Animated, {
      useAnimatedStyle,
      useSharedValue,
    } from 'react-native-reanimated';

    function Box() {
      const offset = useSharedValue(0);

      const animatedStyles = useAnimatedStyle(() => {
        return {
          transform: [{ translateX: offset.value * 255 }],
        };
      });

      return (
        <>
          <Animated.View style={[styles.box, animatedStyles]} />
          <Button onPress={() => (offset.value = Math.random())} title="Move" />
        </>
      );
    }
  `;

    const { code } = executeTransform(input);
    expect(code).toMatchInlineSnapshot(`
    "var _reactNativeReanimated = _interopRequireWildcard(require(\\"react-native-reanimated\\"));

    function _getRequireWildcardCache(nodeInterop) { if (typeof WeakMap !== \\"function\\") return null; var cacheBabelInterop = new WeakMap(); var cacheNodeInterop = new WeakMap(); return (_getRequireWildcardCache = function _getRequireWildcardCache(nodeInterop) { return nodeInterop ? cacheNodeInterop : cacheBabelInterop; })(nodeInterop); }

    function _interopRequireWildcard(obj, nodeInterop) { if (!nodeInterop && obj && obj.__esModule) { return obj; } if (obj === null || typeof obj !== \\"object\\" && typeof obj !== \\"function\\") { return { default: obj }; } var cache = _getRequireWildcardCache(nodeInterop); if (cache && cache.has(obj)) { return cache.get(obj); } var newObj = {}; var hasPropertyDescriptor = Object.defineProperty && Object.getOwnPropertyDescriptor; for (var key in obj) { if (key !== \\"default\\" && Object.prototype.hasOwnProperty.call(obj, key)) { var desc = hasPropertyDescriptor ? Object.getOwnPropertyDescriptor(obj, key) : null; if (desc && (desc.get || desc.set)) { Object.defineProperty(newObj, key, desc); } else { newObj[key] = obj[key]; } } } newObj.default = obj; if (cache) { cache.set(obj, newObj); } return newObj; }

    function Box() {
      var offset = (0, _reactNativeReanimated.useSharedValue)(0);
      var animatedStyles = (0, _reactNativeReanimated.useAnimatedStyle)(function () {
        var _f = function _f() {
          return {
            transform: [{
              translateX: offset.value * 255
            }]
          };
        };

        _f._closure = {
          offset: offset
        };
        _f.asString = \\"function _f(){const{offset}=jsThis._closure;{return{transform:[{translateX:offset.value*255}]};}}\\";
        _f.__workletHash = 7114514849439;
        _f.__location = \\"${process.cwd()}/jest tests fixture (10:48)\\";
        _f.__optimalization = 3;
        return _f;
      }());
      return React.createElement(React.Fragment, null, React.createElement(_reactNativeReanimated.default.View, {
        style: [styles.box, animatedStyles]
      }), React.createElement(Button, {
        onPress: function onPress() {
          return offset.value = Math.random();
        },
        title: \\"Move\\"
      }));
    }"
    `);
  });

  it.skip("supports default ES6 style imports", () => {
    const input = `
      import * as Reanimated from 'react-native-reanimated';

      function Box() {
        const offset = Reanimated.useSharedValue(0);

        const animatedStyles = Reanimated.useAnimatedStyle(() => {
          return {
            transform: [{ translateX: offset.value * 255 }],
          };
        });

        return (
          <>
            <Animated.View style={[styles.box, animatedStyles]} />
            <Button onPress={() => (offset.value = Math.random())} title="Move" />
          </>
        );
      }
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_closure");
  });

  it("doesn't transform functions without 'worklet' directive", () => {
    const input = `
      function f(x) {
        return x + 2;
      }
    `;

    const { code } = executeTransform(input);
    expect(code).not.toContain("_f.__workletHash");
  });

  it("removes comments from worklets", () => {
    const input = `
      const f = () => {
        'worklet';
        // some comment
        /*
        * other comment
        */
        return true;
      };
    `;

    const { code } = executeTransform(input);
    expect(code).not.toContain("some comment");
    expect(code).not.toContain("other comment");
  });

  it('removes "worklet"; directive from worklets', () => {
    const input = `
      function foo(x) {
        "worklet"; // prettier-ignore
        return x + 2;
      }
    `;

    const { code } = executeTransform(input);
    expect(code).not.toContain('\\"worklet\\";');
  });

  it("removes 'worklet'; directive from worklets", () => {
    const input = `
      function foo(x) {
        'worklet'; // prettier-ignore
        return x + 2;
      }
    `;

    const { code } = executeTransform(input);
    expect(code).not.toContain("'worklet';");
  });

  it("doesn't transform string literals", () => {
    const input = `
      function foo(x) {
        'worklet';
        const bar = 'worklet'; // prettier-ignore
        const baz = "worklet"; // prettier-ignore
      }
    `;

    const { code } = executeTransform(input);
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      const foo = function() {
          const _f = function _f(x) {
              ;
              const bar = 'worklet';
              // prettier-ignore
              const baz = \\"worklet\\";
          };
          _f._closure = {};
          _f.asString = 'function foo(x){;const bar=\\"worklet\\";const baz=\\"worklet\\";}';
          _f.__workletHash = 2809676670;
          _f.__location = \\"${process.cwd()}/jest tests fixture (2:6)\\";
          return _f;
      }();
      "
    `);
  });

  it.skip("captures worklets environment", () => {
    const input = `
      const x = 5;

      const objX = { x };

      function f() {
        'worklet';
        return { res: x + objX.x };
      }
    `;

    const { code } = executeTransform(input);
    expect(code).toMatchInlineSnapshot(`
      "var x = 5;
      var objX = {
        x: x
      };

      var f = function () {
        var _f = function _f() {
          return {
            res: x + objX.x
          };
        };

        _f._closure = {
          x: x,
          objX: {
            x: objX.x
          }
        };
        _f.asString = \\"function f(){const{x,objX}=jsThis._closure;{return{res:x+objX.x};}}\\";
        _f.__workletHash = 10184269015616;
        _f.__location = \\"${process.cwd()}/jest tests fixture (6:6)\\";
        return _f;
      }();"
      `);
  });

  /*
  it.skip("doesn't capture globals", () => {
    const input = `
      function f() {
        'worklet';
        console.log('test');
      }
    `;

    const { code, ast } = executeTransform(input, { ast: true });
    let closureBindings;
    traverse(ast, {
      enter(path) {
        if (
          path.isAssignmentExpression() &&
          path.node.left.property.name === '_closure'
        ) {
          closureBindings = path.node.right.properties;
        }
      },
    });
    expect(closureBindings).toEqual([]);
    expect(code).toMatchInlineSnapshot(`
"var f = function () {
  var _f = function _f() {
    console.log('test');
  };

  _f._closure = {};
  _f.asString = \\"function f(){console.log('test');}\\";
  _f.__workletHash = 13298016111221;
  _f.__location = \\"${ process.cwd() }/jest tests fixture (2:6)\\";
  return _f;
}();"
`);
  });*/

  // functions

  it("workletizes FunctionDeclaration", () => {
    const input = `
      function foo(x) {
        'worklet';
        return x + 2;
      }
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).not.toContain('\\"worklet\\";');
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      const foo = function() {
          const _f = function _f(x) {
              ;
              return x + 2;
          };
          _f._closure = {};
          _f.asString = \\"function foo(x){;return x+2;}\\";
          _f.__workletHash = 3468386974;
          _f.__location = \\"${ process.cwd() }/jest tests fixture (2:6)\\";
          return _f;
      }();
      "
    `);
  });

  it("workletizes ArrowFunctionExpression", () => {
    const input = `
      const foo = (x) => {
        'worklet';
        return x + 2;
      };
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).not.toContain('\\"worklet\\";');
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      const foo = function() {
          const _f = function _f(x) {
              ;
              return x + 2;
          };
          _f._closure = {};
          _f.asString = \\"function _f(x){;return x+2;}\\";
          _f.__workletHash = 3611478349;
          _f.__location = \\"${process.cwd()}/jest tests fixture (2:18)\\";
          return _f;
      }();
      "
    `);
  });

  it("workletizes unnamed FunctionExpression", () => {
    const input = `
      const foo = function (x) {
        'worklet';
        return x + 2;
      };
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).not.toContain('\\"worklet\\";');
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      const foo = function() {
          const _f = function _f(x) {
              ;
              return x + 2;
          };
          _f._closure = {};
          _f.asString = \\"function _f(x){;return x+2;}\\";
          _f.__workletHash = 3611478349;
          _f.__location = \\"${process.cwd()}/jest tests fixture (2:18)\\";
          return _f;
      }();
      "
    `);
  });

  it("workletizes named FunctionExpression", () => {
    const input = `
      const foo = function foo(x) {
        'worklet';
        return x + 2;
      };
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).not.toContain('\\"worklet\\";');
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      const foo = function() {
          const _f = function _f(x) {
              ;
              return x + 2;
          };
          _f._closure = {};
          _f.asString = \\"function foo(x){;return x+2;}\\";
          _f.__workletHash = 3468386974;
          _f.__location = \\"${process.cwd()}/jest tests fixture (2:18)\\";
          return _f;
      }();
      "
    `);
  });

  // class methods

  // Note: plugin does not do any downlevel transform for the class method itself.
  // Core transform should be configured to do transform if needed.
  it("workletizes instance method", () => {
    const input = `
      class Foo {
        bar(x) {
          'worklet';
          return x + 2;
        }
      }
    `;

    const { code } = executeTransform(input);

    expect(code).toContain("_f.__workletHash");
    expect(code).not.toContain('\\"worklet\\";');
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      class Foo {
          bar() {
              const _f = function _f(x) {
                  ;
                  return x + 2;
              };
              _f._closure = {};
              _f.asString = \\"function bar(x){;return x+2;}\\";
              _f.__workletHash = 2790860375;
              _f.__location = \\"${process.cwd()}/jest tests fixture (3:8)\\";
              return _f;
          }
      }
      "
    `);
  });

  it.skip("workletizes static method", () => {
    const input = `
      class Foo {
        static bar(x) {
          'worklet';
          return x + 2;
        }
      }
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).not.toContain('\\"worklet\\";');
    expect(code).toMatchInlineSnapshot(`
    "var _interopRequireDefault = require(\\"@babel/runtime/helpers/interopRequireDefault\\");

    var _classCallCheck2 = _interopRequireDefault(require(\\"@babel/runtime/helpers/classCallCheck\\"));

    var _createClass2 = _interopRequireDefault(require(\\"@babel/runtime/helpers/createClass\\"));

    var Foo = function () {
      function Foo() {
        (0, _classCallCheck2.default)(this, Foo);
      }

      (0, _createClass2.default)(Foo, null, [{
        key: \\"bar\\",
        value: function () {
          var _f = function _f(x) {
            return x + 2;
          };

          _f._closure = {};
          _f.asString = \\"function bar(x){return x+2;}\\";
          _f.__workletHash = 16974800582491;
          _f.__location = \\"${process.cwd()}/jest tests fixture\\";
          return _f;
        }()
      }]);
      return Foo;
    }();"
    `);
  });

  it.skip("workletizes getter", () => {
    const input = `
      class Foo {
        get bar() {
          'worklet';
          return x + 2;
        }
      }
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).not.toContain('\\"worklet\\";');
    expect(code).toMatchInlineSnapshot(`
    "var _interopRequireDefault = require(\\"@babel/runtime/helpers/interopRequireDefault\\");

    var _classCallCheck2 = _interopRequireDefault(require(\\"@babel/runtime/helpers/classCallCheck\\"));

    var _createClass2 = _interopRequireDefault(require(\\"@babel/runtime/helpers/createClass\\"));

    var Foo = function () {
      function Foo() {
        (0, _classCallCheck2.default)(this, Foo);
      }

      (0, _createClass2.default)(Foo, [{
        key: \\"bar\\",
        get: function () {
          var _f = function _f() {
            return x + 2;
          };

          _f._closure = {
            x: x
          };
          _f.asString = \\"function get(){const{x}=jsThis._closure;{return x+2;}}\\";
          _f.__workletHash = 2468276954688;
          _f.__location = \\"${process.cwd()}/jest tests fixture\\";
          return _f;
        }()
      }]);
      return Foo;
    }();"
    `);
  });

  // function hooks

  it.skip("workletizes hook wrapped ArrowFunctionExpression automatically", () => {
    const input = `
      const animatedStyle = useAnimatedStyle(() => ({
        width: 50,
      }));
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).toMatchInlineSnapshot(`
    "var animatedStyle = useAnimatedStyle(function () {
      var _f = function _f() {
        return {
          width: 50
        };
      };

      _f._closure = {};
      _f.asString = \\"function _f(){return{width:50};}\\";
      _f.__workletHash = 9756190407413;
      _f.__location = \\"${process.cwd()}/jest tests fixture (2:45)\\";
      _f.__optimalization = 3;
      return _f;
    }());"
    `);
  });

  it.skip("workletizes hook wrapped unnamed FunctionExpression automatically", () => {
    const input = `
      const animatedStyle = useAnimatedStyle(function () {
        return {
          width: 50,
        };
      });
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).toMatchInlineSnapshot(`
    "var animatedStyle = useAnimatedStyle(function () {
      var _f = function _f() {
        return {
          width: 50
        };
      };

      _f._closure = {};
      _f.asString = \\"function _f(){return{width:50};}\\";
      _f.__workletHash = 9756190407413;
      _f.__location = \\"${process.cwd()}/jest tests fixture (2:45)\\";
      _f.__optimalization = 3;
      return _f;
    }());"
    `);
  });

  it.skip("workletizes hook wrapped named FunctionExpression automatically", () => {
    const input = `
      const animatedStyle = useAnimatedStyle(function foo() {
        return {
          width: 50,
        };
      });
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).toMatchInlineSnapshot(`
    "var animatedStyle = useAnimatedStyle(function () {
      var _f = function _f() {
        return {
          width: 50
        };
      };

      _f._closure = {};
      _f.asString = \\"function foo(){return{width:50};}\\";
      _f.__workletHash = 6275510763626;
      _f.__location = \\"${process.cwd()}/jest tests fixture (2:45)\\";
      _f.__optimalization = 3;
      return _f;
    }());"
    `);
  });

  // object hooks

  it("workletizes object hook wrapped ArrowFunctionExpression automatically", () => {
    const input = `
      useAnimatedGestureHandler({
        onStart: (event) => {
          console.log(event);
        },
      });
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).toMatchInlineSnapshot(`
    "\\"use strict\\";
    useAnimatedGestureHandler({
        onStart: function() {
            const _f = function _f(event) {
                console.log(event);
            };
            _f._closure = {};
            _f.asString = \\"function _f(event){console.log(event);}\\";
            _f.__workletHash = 4276664511;
            _f.__location = \\"${process.cwd()}/jest tests fixture (3:17)\\";
            return _f;
        }()
    });
    "
  `);
  });

  it("workletizes object hook wrapped unnamed FunctionExpression automatically", () => {
    const input = `
      useAnimatedGestureHandler({
        onStart: function (event) {
          console.log(event);
        },
      });
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      useAnimatedGestureHandler({
          onStart: function() {
              const _f = function _f(event) {
                  console.log(event);
              };
              _f._closure = {};
              _f.asString = \\"function _f(event){console.log(event);}\\";
              _f.__workletHash = 4276664511;
              _f.__location = \\"${process.cwd()}/jest tests fixture (3:17)\\";
              return _f;
          }()
      });
      "
    `);
  });

  it("workletizes object hook wrapped named FunctionExpression automatically", () => {
    const input = `
      useAnimatedGestureHandler({
        onStart: function onStart(event) {
          console.log(event);
        },
      });
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      useAnimatedGestureHandler({
          onStart: function() {
              const _f = function _f(event) {
                  console.log(event);
              };
              _f._closure = {};
              _f.asString = \\"function onStart(event){console.log(event);}\\";
              _f.__workletHash = 1675048407;
              _f.__location = \\"${process.cwd()}/jest tests fixture (3:17)\\";
              return _f;
          }()
      });
      "
    `);
  });

  it("workletizes object hook wrapped ObjectMethod automatically", () => {
    const input = `
      useAnimatedGestureHandler({
        onStart(event) {
          console.log(event);
        },
      });
    `;

    const { code } = executeTransform(input);
    expect(code).toContain("_f.__workletHash");
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      useAnimatedGestureHandler({
          onStart: function() {
              const _f = function _f(event) {
                  console.log(event);
              };
              _f._closure = {};
              _f.asString = \\"function onStart(event){console.log(event);}\\";
              _f.__workletHash = 1675048407;
              _f.__location = \\"${process.cwd()}/jest tests fixture (3:8)\\";
              return _f;
          }
      });
      "
    `);
  });

  it("supports empty object in hooks", () => {
    const input = `
      useAnimatedGestureHandler({});
    `;

    executeTransform(input);
  });

  it("transforms each object property in hooks", () => {
    const input = `
      useAnimatedGestureHandler({
        onStart: () => {},
        onUpdate: () => {},
        onEnd: () => {},
      });
    `;

    const { code } = executeTransform(input);
    expect(code).toMatch(/^(.*)(_f\.__workletHash(.*)){3}$/s);
  });

  // React Native Gesture Handler

  it("workletizes possibly chained gesture object callback functions automatically", () => {
    const input = `
      import { Gesture } from 'react-native-gesture-handler';

      const foo = Gesture.Tap()
        .numberOfTaps(2)
        .onBegin(() => {
          console.log('onBegin');
        })
        .onStart((_event) => {
          console.log('onStart');
        })
        .onEnd((_event, _success) => {
          console.log('onEnd');
        });
    `;

    const { code } = executeTransform(input);
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      var _reactNativeGestureHandler = require(\\"react-native-gesture-handler\\");
      const foo = _reactNativeGestureHandler.Gesture.Tap().numberOfTaps(2).onBegin(function() {
          const _f = function _f() {
              console.log('onBegin');
          };
          _f._closure = {};
          _f.asString = 'function _f(){console.log(\\"onBegin\\");}';
          _f.__workletHash = 498039665;
          _f.__location = \\"${process.cwd()}/jest tests fixture (6:17)\\";
          return _f;
      }()).onStart(function() {
          const _f = function _f(_event) {
              console.log('onStart');
          };
          _f._closure = {};
          _f.asString = 'function _f(_event){console.log(\\"onStart\\");}';
          _f.__workletHash = 2816630107;
          _f.__location = \\"${process.cwd()}/jest tests fixture (9:17)\\";
          return _f;
      }()).onEnd(function() {
          const _f = function _f(_event, _success) {
              console.log('onEnd');
          };
          _f._closure = {};
          _f.asString = 'function _f(_event,_success){console.log(\\"onEnd\\");}';
          _f.__workletHash = 1652342874;
          _f.__location = \\"${process.cwd()}/jest tests fixture (12:15)\\";
          return _f;
      }());
      "
    `);
  });

  it("doesn't transform standard callback functions", () => {
    const input = `
      const foo = Something.Tap().onEnd((_event, _success) => {
        console.log('onEnd');
      });
    `;

    const { code } = executeTransform(input);
    expect(code).toMatchInlineSnapshot(`
      "\\"use strict\\";
      const foo = Something.Tap().onEnd((_event, _success)=>{
          console.log('onEnd');
      });
      "
    `);
  });

  it.skip("transforms spread operator in worklets for arrays", () => {
    const input = `
      function foo() {
        'worklet';
        const bar = [4, 5];
        const baz = [1, ...[2, 3], ...bar];
      }
    `;

    const { code } = executeTransform(input);
    expect(code).toMatchInlineSnapshot(`
    "var foo = function () {
      var _f = function _f() {
        var bar = [4, 5];
        var baz = [1].concat([2, 3], bar);
      };

      _f._closure = {};
      _f.asString = \\"function foo(){const bar=[4,5];const baz=[1,...[2,3],...bar];}\\";
      _f.__workletHash = 3161057533258;
      _f.__location = \\"${process.cwd()}/jest tests fixture (2:6)\\";
      return _f;
    }();"
    `);
  });

  it.skip("transforms spread operator in worklets for objects", () => {
    const input = `
      function foo() {
        'worklet';
        const bar = {d: 4, e: 5};
        const baz = { a: 1, ...{ b: 2, c: 3 }, ...bar };
      }
    `;

    const { code } = executeTransform(input);
    expect(code).toMatchInlineSnapshot(`
    "var _interopRequireDefault = require(\\"@babel/runtime/helpers/interopRequireDefault\\");

    var _extends2 = _interopRequireDefault(require(\\"@babel/runtime/helpers/extends\\"));

    var foo = function () {
      var _f = function _f() {
        var bar = {
          d: 4,
          e: 5
        };
        var baz = (0, _extends2.default)({
          a: 1
        }, {
          b: 2,
          c: 3
        }, {}, bar);
      };

      _f._closure = {};
      _f.asString = \\"function foo(){const bar={d:4,e:5};const baz={a:1,...{b:2,c:3},...bar};}\\";
      _f.__workletHash = 792186851025;
      _f.__location = \\"${process.cwd()}/jest tests fixture (2:6)\\";
      return _f;
    }();"
    `);
  });

  it.skip("transforms spread operator in worklets for function arguments", () => {
    const input = `
      function foo(...args) {
        'worklet';
        console.log(args);
      }
    `;

    const { code } = executeTransform(input);
    expect(code).toMatchInlineSnapshot(`
    "var foo = function () {
      var _f = function _f() {
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }

        console.log(args);
      };

      _f._closure = {};
      _f.asString = \\"function foo(...args){console.log(args);}\\";
      _f.__workletHash = 9866931756941;
      _f.__location = \\"${process.cwd()}/jest tests fixture (2:6)\\";
      return _f;
    }();"
    `);
  });

  it.skip("transforms spread operator in worklets for function calls", () => {
    const input = `
      function foo(arg) {
        'worklet';
        console.log(...arg);
      }
    `;

    const { code } = executeTransform(input);
    expect(code).toMatchInlineSnapshot(`
    "var _interopRequireDefault = require(\\"@babel/runtime/helpers/interopRequireDefault\\");

    var _toConsumableArray2 = _interopRequireDefault(require(\\"@babel/runtime/helpers/toConsumableArray\\"));

    var foo = function () {
      var _f = function _f(arg) {
        var _console;

        (_console = console).log.apply(_console, (0, _toConsumableArray2.default)(arg));
      };

      _f._closure = {};
      _f.asString = \\"function foo(arg){console.log(...arg);}\\";
      _f.__workletHash = 2015887751437;
      _f.__location = \\"${process.cwd()}/jest tests fixture (2:6)\\";
      return _f;
    }();"
    `);
  });
});
