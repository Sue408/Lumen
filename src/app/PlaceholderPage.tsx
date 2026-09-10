type PlaceholderPageProps = {
  title: string;
};

export function PlaceholderPage({ title }: PlaceholderPageProps) {
  return (
    <main className="usage-page">
      <header className="page-header">
        <div>
          <div className="eyebrow">LUMEN LEDGER</div>
          <h1>{title}</h1>
        </div>
      </header>
      <p className="usage-summary">这一页还在装订中，敬请期待。</p>
    </main>
  );
}
