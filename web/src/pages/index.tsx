// import { useRouter } from 'next/router';

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
      <h1>HelloWorld</h1>
    </MUI>
  );
};

export default Index;
