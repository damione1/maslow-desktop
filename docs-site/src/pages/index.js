import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';

import Heading from '@theme/Heading';
import styles from './index.module.css';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link className="button button--secondary button--lg margin-horiz--sm" to="/docs/intro">
            App overview
          </Link>
          <Link className="button button--secondary button--lg margin-horiz--sm" to="/docs/api/using-the-api">
            Using the API
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout title={siteConfig.title} description={siteConfig.tagline}>
      <HomepageHeader />
      <main className="container margin-vert--lg">
        <div className="row">
          <div className="col col--4">
            <Heading as="h3">The app</Heading>
            <p>
              What Maslow Desktop is, how it's laid out, and how to build and run it. See{' '}
              <Link to="/docs/intro">App overview</Link>.
            </p>
          </div>
          <div className="col col--4">
            <Heading as="h3">Control API</Heading>
            <p>
              A gRPC service, an HTTP/JSON gateway, and an MCP server all expose the same machine operations. See{' '}
              <Link to="/docs/api/using-the-api">Using the API</Link> for auth, then the HTTP, gRPC, and MCP
              references.
            </p>
          </div>
          <div className="col col--4">
            <Heading as="h3">Generated, not hand-written</Heading>
            <p>
              The HTTP/gRPC/MCP reference pages are generated from this repo's real proto files, route
              registrations, and live MCP tool registry on every docs build, so they can't drift from what the app
              actually does.
            </p>
          </div>
        </div>
      </main>
    </Layout>
  );
}
