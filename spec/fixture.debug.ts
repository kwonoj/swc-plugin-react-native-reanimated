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
          _f.__workletHash = 3611478349;
          _f.__location = \\"${process.cwd()}/jest tests fixture (2:18)\\";
          return _f;
      }();
      "
    `);
  });
});
