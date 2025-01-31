import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Container } from "@/components/container";

export default function Home() {
  return (
    <Container className="py-8">
      <Card>
        <CardHeader>
          <CardTitle>TinyCongress</CardTitle>
          <CardDescription>Decentralized governance platform</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex gap-4">
            <Button>Create Poll</Button>
            <Button variant="outline">View Results</Button>
          </div>
          
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            <Card>
              <CardHeader>
                <CardTitle>Active Polls</CardTitle>
              </CardHeader>
              <CardContent>
                <p>No active polls</p>
              </CardContent>
            </Card>
            
            <Card>
              <CardHeader>
                <CardTitle>Recent Votes</CardTitle>
              </CardHeader>
              <CardContent>
                <p>No recent votes</p>
              </CardContent>
            </Card>
            
            <Card>
              <CardHeader>
                <CardTitle>Trust Scores</CardTitle>
              </CardHeader>
              <CardContent>
                <p>No trust scores available</p>
              </CardContent>
            </Card>
          </div>
        </CardContent>
      </Card>
    </Container>
  );
}
