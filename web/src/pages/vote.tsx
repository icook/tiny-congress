// import { useRouter } from 'next/router';

import TextField from '@mui/material/TextField';
import Paper from '@mui/material/Paper';
// import Card from '@mui/material/Card';
// import CardContent from '@mui/material/CardContent';
import Typography from '@mui/material/Typography';

import Link from '../../src/Link';
import { Meta } from '@/layouts/Meta';
import { MUI } from '@/templates/MUI';

const Voter = () => {
  return (
    <Paper variant="outlined" square>
      <Typography variant="h5" component="div">
        Vote
      </Typography>
      <TextField id="topic" label="Topic" variant="outlined" />
    </Paper>
  )
}

const Index = () => {
  // const router = useRouter();

  return (
    <MUI
      meta={
        <Meta
          title="Vote"
          description=""
        />
      }
    >
      <Voter />
    </MUI>
  );
};

export default Index;
