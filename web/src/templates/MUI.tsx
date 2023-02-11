import * as React from 'react';
import Container from '@mui/material/Container';
import MenuIcon from '@mui/icons-material/Menu';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import { AppBar, Toolbar, IconButton } from '@mui/material';

type IMUIProps = {
  meta: React.ReactNode;
  children: React.ReactNode;
};

const MUI = (props: IMUIProps) => (
    <Container maxWidth="lg">
      {props.meta}
      <AppBar position="static">
        <Toolbar variant="dense">
          <IconButton edge="start" color="inherit" aria-label="menu" sx={{ mr: 2 }}>
            <MenuIcon />
          </IconButton>
          <Typography variant="h6" color="inherit" component="div">
            TinyCongress
          </Typography>
        </Toolbar>
      </AppBar>
      <Box
        sx={{
          my: 4,
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'center',
          alignItems: 'center',
        }}
      >
        {props.children}
      </Box>
    </Container>
  );

export { MUI };
