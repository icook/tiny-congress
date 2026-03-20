import { render, screen, userEvent } from '@test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BotTrace } from '../api';
import { BotTraceViewer } from './BotTraceViewer';

const NOW = new Date('2026-03-19T12:00:00Z');

const completedTrace: BotTrace = {
  id: 'trace-1',
  task: 'research_company',
  run_mode: 'iterate',
  steps: [
    {
      type: 'exa_search',
      query: 'Apple Inc labor practices 2024',
      latency_ms: 890,
      cost_usd: 0.001,
      cache: { nginx_hit: true },
      output_summary: '10 results for Labor Practices',
    },
    {
      type: 'exa_search',
      query: 'Apple Inc environmental impact 2024',
      latency_ms: 450,
      cost_usd: 0.001,
      cache: { nginx_hit: false },
      output_summary: '8 results for Environmental Impact',
    },
    {
      type: 'llm_synthesis',
      model: 'anthropic/claude-sonnet-4-20250514',
      prompt_tokens: 1200,
      completion_tokens: 800,
      latency_ms: 2340,
      cost_usd: 0.003,
      cache: { openrouter_prompt_tokens_cached: 400, litellm_proxy_hit: false },
      output_summary: 'Generated 4 evidence items for Apple Inc.',
    },
  ],
  total_cost_usd: 0.005,
  status: 'completed',
  error: null,
  created_at: '2026-03-19T11:58:00Z',
  completed_at: '2026-03-19T11:58:30Z',
};

const failedTrace: BotTrace = {
  id: 'trace-2',
  task: 'research_company',
  run_mode: 'iterate',
  steps: [
    {
      type: 'exa_search',
      query: 'Tesla Inc governance 2024',
      latency_ms: 5000,
      cost_usd: 0,
      cache: {},
      output_summary: '',
    },
  ],
  total_cost_usd: 0,
  status: 'failed',
  error: 'Exa API timeout after 5000ms',
  created_at: '2026-03-19T11:55:00Z',
  completed_at: '2026-03-19T11:55:05Z',
};

describe('BotTraceViewer', () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders nothing when traces array is empty', () => {
    render(<BotTraceViewer traces={[]} />);
    expect(screen.queryByText('Research Trace')).not.toBeInTheDocument();
    expect(screen.queryByText('Bot generated')).not.toBeInTheDocument();
  });

  it('renders Research Trace header when traces exist', () => {
    render(<BotTraceViewer traces={[completedTrace]} />);
    expect(screen.getByText('Research Trace')).toBeInTheDocument();
  });

  it('shows collapsed summary with step count, cost, and relative time', () => {
    render(<BotTraceViewer traces={[completedTrace]} />);
    expect(screen.getByText('Bot generated')).toBeInTheDocument();
    expect(screen.getByText(/3 steps/)).toBeInTheDocument();
    expect(screen.getByText(/\$0\.005/)).toBeInTheDocument();
    // "2 minutes ago" from the completed_at timestamp
    expect(screen.getByText(/2 minutes ago|1 minute ago/)).toBeInTheDocument();
  });

  it('shows status badge', () => {
    render(<BotTraceViewer traces={[completedTrace]} />);
    expect(screen.getByText('completed')).toBeInTheDocument();
  });

  it('shows failed status and error message on expand', async () => {
    const user = userEvent.setup();
    render(<BotTraceViewer traces={[failedTrace]} />);

    expect(screen.getByText('failed')).toBeInTheDocument();

    // Expand
    await user.click(screen.getByText('Bot generated'));
    expect(screen.getByText(/Exa API timeout after 5000ms/)).toBeInTheDocument();
  });

  it('expands to show timeline with human-readable step types', async () => {
    const user = userEvent.setup();
    render(<BotTraceViewer traces={[completedTrace]} />);

    await user.click(screen.getByText('Bot generated'));

    // Step type labels
    expect(screen.getAllByText('Web Search')).toHaveLength(2);
    expect(screen.getByText('LLM Synthesis')).toBeInTheDocument();
  });

  it('shows short model name without provider prefix', async () => {
    const user = userEvent.setup();
    render(<BotTraceViewer traces={[completedTrace]} />);

    await user.click(screen.getByText('Bot generated'));

    expect(screen.getByText('claude-sonnet-4-20250514')).toBeInTheDocument();
    expect(screen.queryByText('anthropic/claude-sonnet-4-20250514')).not.toBeInTheDocument();
  });

  it('shows cached badge only when a cache value is truthy', async () => {
    const user = userEvent.setup();
    render(<BotTraceViewer traces={[completedTrace]} />);

    await user.click(screen.getByText('Bot generated'));

    // Two "cached" badges expected:
    // - exa_search step 1: nginx_hit: true -> cached
    // - llm_synthesis step: openrouter_prompt_tokens_cached: 400 -> cached (truthy)
    // exa_search step 2: nginx_hit: false -> NOT cached
    const cachedBadges = screen.getAllByText('cached');
    expect(cachedBadges).toHaveLength(2);
  });

  it('shows search query in italic when present', async () => {
    const user = userEvent.setup();
    render(<BotTraceViewer traces={[completedTrace]} />);

    await user.click(screen.getByText('Bot generated'));

    expect(screen.getByText(/Apple Inc labor practices 2024/)).toBeInTheDocument();
  });

  it('shows token counts and cost per step', async () => {
    const user = userEvent.setup();
    render(<BotTraceViewer traces={[completedTrace]} />);

    await user.click(screen.getByText('Bot generated'));

    // LLM step tokens
    expect(screen.getByText(/1200 \+ 800 tokens/)).toBeInTheDocument();
    // LLM step cost
    expect(screen.getByText('$0.0030')).toBeInTheDocument();
  });

  it('expands step detail to show output summary', async () => {
    const user = userEvent.setup();
    render(<BotTraceViewer traces={[completedTrace]} />);

    // Expand trace card
    await user.click(screen.getByText('Bot generated'));

    // Click "Show output" on the LLM step
    const showButtons = screen.getAllByText('Show output');
    expect(showButtons.length).toBeGreaterThan(0);

    await user.click(showButtons[showButtons.length - 1]);

    expect(screen.getByText('Generated 4 evidence items for Apple Inc.')).toBeInTheDocument();
  });

  it('collapses step detail', async () => {
    const user = userEvent.setup();
    render(<BotTraceViewer traces={[completedTrace]} />);

    await user.click(screen.getByText('Bot generated'));

    const showButtons = screen.getAllByText('Show output');
    await user.click(showButtons[showButtons.length - 1]);

    expect(screen.getByText('Hide output')).toBeInTheDocument();
    await user.click(screen.getByText('Hide output'));

    // "Show output" should be back
    expect(screen.getAllByText('Show output').length).toBeGreaterThan(0);
  });

  it('renders multiple traces', () => {
    render(<BotTraceViewer traces={[completedTrace, failedTrace]} />);

    const botLabels = screen.getAllByText('Bot generated');
    expect(botLabels).toHaveLength(2);
  });
});
