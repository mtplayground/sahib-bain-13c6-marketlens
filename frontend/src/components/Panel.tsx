import type { ReactNode } from 'react';

type PanelTone = 'default' | 'accent' | 'warning';

type PanelProps = {
  title: string;
  eyebrow?: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
  tone?: PanelTone;
};

export function Panel({
  title,
  eyebrow,
  actions,
  children,
  className,
  tone = 'default'
}: PanelProps) {
  const panelClassName = ['terminal-panel', `terminal-panel--${tone}`, className]
    .filter(Boolean)
    .join(' ');

  return (
    <section className={panelClassName}>
      <header className="terminal-panel__header">
        <div>
          {eyebrow ? <div className="terminal-panel__eyebrow">{eyebrow}</div> : null}
          <h2>{title}</h2>
        </div>
        {actions ? <div className="terminal-panel__actions">{actions}</div> : null}
      </header>
      <div className="terminal-panel__body">{children}</div>
    </section>
  );
}
