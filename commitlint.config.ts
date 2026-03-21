export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "type-enum": [
      2,
      "always",
      ["feat", "fix", "docs", "style", "refactor", "test", "chore", "ci", "perf", "migration"],
    ],
    "scope-enum": [
      1,
      "always",
      [
        "web",
        "mobile",
        "domain",
        "types",
        "validation",
        "supabase",
        "ci",
        "docs",
        "config",
        "tokens",
      ],
    ],
  },
};
