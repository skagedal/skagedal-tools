import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactPlugin from "eslint-plugin-react";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";

export default tseslint.config(
  { ignores: ["dist/**", "node_modules/**"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  reactPlugin.configs.flat.recommended,
  reactPlugin.configs.flat["jsx-runtime"],
  {
    plugins: { "react-hooks": reactHooks },
    rules: reactHooks.configs.recommended.rules,
  },
  {
    languageOptions: {
      globals: { ...globals.node, ...globals.browser },
    },
    settings: { react: { version: "detect" } },
  },
  // codestyle/react.md: dates are Temporal, and effects live in named hooks.
  {
    rules: {
      "no-restricted-globals": ["error", { name: "Date", message: "Use Temporal; see codestyle/react.md." }],
      "no-restricted-imports": [
        "error",
        {
          paths: [{
            name: "react",
            importNames: ["useEffect", "useLayoutEffect"],
            message: "Fetch with react-query, use a hook library, or write a named hook in hooks.ts; see codestyle/react.md.",
          }],
        },
      ],
    },
  },
  { files: ["src/hooks.ts"], rules: { "no-restricted-imports": "off" } },
);
