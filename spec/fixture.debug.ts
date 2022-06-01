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
  /*
    ['plugin', (code: string) => {
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
      }

      const { transformSync } = require('@swc/core');
      return transformSync(code, opt)
    }],*/
  [
    "custom transform",
    (code: string) => {
      const { transformSync } = require("../index");
      return transformSync(code, true, Buffer.from(JSON.stringify(options)));
    },
  ],
];

describe.each(transformPresets)("fixture with %s", (_, executeTransform) => {
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

    const foo = _reactNativeGestureHandler.Gesture.Tap().numberOfTaps(2).onBegin(function () {
    var _f = function _f() {
        console.log('onBegin');
    };

        _f._closure = {};
        _f.asString = \\"function _f(){console.log('onBegin');}\\";
        _f.__workletHash = 13662490049850;
        _f.__location = \\"${process.cwd()}/jest tests fixture (6:17)\\";
        return _f;
      }()).onStart(function () {
        var _f = function _f(_event) {
          console.log('onStart');
        };

        _f._closure = {};
        _f.asString = \\"function _f(_event){console.log('onStart');}\\";
        _f.__workletHash = 16334902412526;
        _f.__location = \\"${process.cwd()}/jest tests fixture (9:17)\\";
        return _f;
      }()).onEnd(function () {
        var _f = function _f(_event, _success) {
          console.log('onEnd');
        };

        _f._closure = {};
        _f.asString = \\"function _f(_event,_success){console.log('onEnd');}\\";
        _f.__workletHash = 4053780716017;
        _f.__location = \\"${process.cwd()}/jest tests fixture (12:15)\\";
        return _f;
      }());"
    `);
  });
});
