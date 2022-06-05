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
  it("transforms spread operator in worklets for arrays", () => {
    const input = `
      function foo() {
        'worklet';
        const bar = [4, 5];
        const baz = [1, ...[2, 3], ...bar];
      }
    `;

    const { code } = executeTransform(input);

    console.log(code);
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
});
