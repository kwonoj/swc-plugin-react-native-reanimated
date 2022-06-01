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
    "var _interopRequireDefault = require(\\"@babel/runtime/helpers/interopRequireDefault\\");

    var _classCallCheck2 = _interopRequireDefault(require(\\"@babel/runtime/helpers/classCallCheck\\"));

    var _createClass2 = _interopRequireDefault(require(\\"@babel/runtime/helpers/createClass\\"));

    var Foo = function () {
      function Foo() {
        (0, _classCallCheck2.default)(this, Foo);
      }

      (0, _createClass2.default)(Foo, [{
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
});
