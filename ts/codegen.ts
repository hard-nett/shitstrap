import codegen from "@cosmwasm/ts-codegen";

codegen({
  contracts: [
    {
      name: "ShitStrap",
      dir: "../schema",
    },
    {
      name: "Cw20Base",
      dir: "../cw20",
    },
  ],
  outPath: "./src/",

  // options are completely optional ;)
  options: {
    bundle: {
      bundleFile: "index.ts",
      scope: "contracts",
    },
    types: {
      enabled: true,
    },
    client: {
      enabled: true,
    },
    reactQuery: {
      enabled: false,
      optionalClient: true,
      version: "v4",
      mutations: true,
      queryKeys: true,
    },
    recoil: {
      enabled: false,
    },
    messageComposer: {
      enabled: true,
    },
  },
}).then(() => {
  console.log("✨ all done!");
});
