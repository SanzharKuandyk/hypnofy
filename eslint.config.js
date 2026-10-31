import ts from "typescript-eslint";
import svelte from "eslint-plugin-svelte";

export default ts.config(
  { ignores: ["src/shared/rpc/generated/**", ".b/**", "dist/**", "node_modules/**"] },
  ...ts.configs.recommended,
  ...svelte.configs["flat/recommended"],
  { files: ["**/*.svelte.ts"], languageOptions: { parser: ts.parser } },
  { files: ["**/*.svelte"], languageOptions: { parserOptions: { parser: ts.parser } } },
  {
    files: ["src/**/*.{ts,svelte}"],
    languageOptions: {
      globals: {
        window: "readonly",
        document: "readonly",
        navigator: "readonly",
        crypto: "readonly",
        setTimeout: "readonly",
        clearTimeout: "readonly",
        setInterval: "readonly",
        clearInterval: "readonly",
        matchMedia: "readonly",
        localStorage: "readonly",
        console: "readonly",
        structuredClone: "readonly",
        HTMLDivElement: "readonly",
        KeyboardEvent: "readonly",
      },
    },
    rules: {
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-non-null-assertion": "error",
      "svelte/no-at-html-tags": "error",
      "svelte/require-each-key": "error",
    },
  },
);
