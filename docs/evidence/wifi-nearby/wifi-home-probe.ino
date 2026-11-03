// Joins the network "Home" with a password and reports whether it connected within 20 s. With
// `--wifi ssid=Home,psk=home-pass-123` it connects; with another access point and Home only a
// network around (`--wifi-nearby ssid=Home`), the join times out.
#include <WiFi.h>

void setup() {
  Serial.begin(115200);
  Serial.println("Ready");
  WiFi.begin("Home", "home-pass-123");
  unsigned long t0 = millis();
  while (WiFi.status() != WL_CONNECTED && millis() - t0 < 20000) delay(100);
  if (WiFi.status() == WL_CONNECTED)
    Serial.printf("connected ip=%s\n", WiFi.localIP().toString().c_str());
  else
    Serial.printf("not connected: status=%d\n", (int)WiFi.status());
}

void loop() { delay(1000); }
