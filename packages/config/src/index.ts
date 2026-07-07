// App-runtime entry — what the web shell + db tooling import. Deploy-time (CDK synth) config is the
// separate '@vegify/config/deploy' entry, imported only by the infra, so app bundles never pull the
// deploy placeholders in.
export * from "./runtime"
