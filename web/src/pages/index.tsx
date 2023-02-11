// import { useRouter } from 'next/router';

import Link from '../../src/Link';
import { Meta } from '@/layouts/Meta';
import { MUI } from '@/templates/MUI';

const Index = () => {
  // const router = useRouter();

  return (
    <MUI
      meta={
        <Meta
          title="Next.js Boilerplate Presentation"
          description="Next js Boilerplate is the perfect starter code for your project. Build your React application with the Next.js framework."
        />
      }
    >
      <Link href="/vote">Vote Mockup</Link>
      <Link href="/vote-2">Vote Mockup 2</Link>
    </MUI>
  );
};

export default Index;
