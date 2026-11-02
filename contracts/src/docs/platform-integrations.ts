import type { DocPage } from './model.js';

/** Overview chooser for platform how-tos (TII-191 / TII-201). */
export const platformIntegrations: DocPage = {
  family: 'sdks',
  version: 'v1',
  slug: 'integrations',
  path: '/docs/integrations',
  title: 'Wrap Pipedream, LangChain, Nango and Mastra',
  navTitle: 'Third-party integrations',
  published: true,
  sections: [
    {
      heading: 'When to use platform adapters',
      aliases: ['when-to-use'],
      text: 'Use these guides when your agent already calls a vendor through Pipedream, LangChain, Nango, Mastra or a similar tool/client shape and you want to wrap that boundary with the embedded Node SDK. Prefer this after the quickstart has wrapped one owned API read. If you still need to choose between the embedded SDK, an HTTP guard host, MCP evidence reads or the browser playground, start with the transport chooser instead. Python supplies callable decorators only; it has no framework adapters.',
      links: [
        { label: 'Choose transport (embedded, HTTP, MCP, playground)', href: '/docs/choose-integration' },
        { label: 'Quickstart', href: '/docs/quickstart' },
        { label: 'Python callable integration', href: '/docs/sdk/python' },
      ],
    },
    {
      heading: 'Recommended try order',
      aliases: ['try-order', 'adoption-order'],
      text: 'Try platforms in this order when choosing where to wrap first: Pipedream → LangChain → Nango → Mastra. That is AGINDOW’s build and setup priority — easiest npm wrap first, then where many agents already live, then Nango’s no-npm sandbox path, then Mastra’s thinner execute-shape coverage. It is not a claim that those platforms officially partner with or endorse AGINDOW. Open the matching platform page for adapter examples and limits.',
      links: [
        { label: 'Pipedream', href: '/docs/integrations/pipedream' },
        { label: 'LangChain', href: '/docs/integrations/langchain' },
        { label: 'Nango', href: '/docs/integrations/nango' },
        { label: 'Mastra', href: '/docs/integrations/mastra' },
        { label: 'Public integrations overview', href: '/integrations' },
        { label: 'Local adapter matrix', href: '/docs/guides/frameworks' },
      ],
    },
    {
      heading: 'What to expect after wrapping',
      aliases: ['what-to-expect', 'expectations'],
      text: 'Observe mode preserves original payloads, results and upstream exceptions. Installing and wrapping alone does not configure detection: the statistical guest is default; configure windows to detect. An empty `recent()` history is not a clean verdict. Selective repair stays off by default and needs reviewed rename rules plus independent verification. Python remains callable-only for these platforms.',
      links: [
        { label: 'Start in observe mode', href: '/docs/guides/observe-first' },
        { label: 'Configure detection windows', href: '/docs/sdk/configuration' },
        { label: 'Detection quickstart', href: '/docs/detection' },
      ],
    },
    {
      heading: 'Reference',
      aliases: ['continue', 'next-steps'],
      text: 'Use the language guides for full call wrapping, the configuration page for selection and repair options, and the matrix for per-framework availability. Public marketing pages summarize surfaces without replacing these how-tos.',
      links: [
        { label: 'Node / TypeScript', href: '/docs/sdk/node' },
        { label: 'Python', href: '/docs/sdk/python' },
        { label: 'Configuration', href: '/docs/sdk/configuration' },
        { label: 'Framework matrix', href: '/docs/guides/frameworks' },
        { label: 'CLI and offline reports', href: '/docs/sdk/cli' },
        { label: 'Integrations overview', href: '/integrations' },
      ],
    },
  ],
};
