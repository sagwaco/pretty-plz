# Authentication

Run the guided setup once after installing:

```
plz configure
```

Or manage the pieces separately:

```
plz login                  # interactive provider picker
plz login anthropic        # Claude OAuth
plz login openai           # paste an OpenAI API key
plz login chatgpt          # ChatGPT OAuth
plz status
plz logout <provider>
```

You can also use environment variables:

```
export ANTHROPIC_API_KEY=sk-ant-...
export OPENAI_API_KEY=sk-...
```

Credential priority is env var, pasted API key, then stored OAuth tokens.
ChatGPT OAuth is owned by `plz`, like Claude account auth: `plz` opens the
browser, receives the ChatGPT callback locally, stores tokens under
`<config_dir>/oauth/chatgpt.json`, and refreshes them on use. Override a call
with `--provider`, `--model`, or `--auth auto|api|oauth`.
