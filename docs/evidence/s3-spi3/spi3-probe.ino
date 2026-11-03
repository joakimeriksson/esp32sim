// Two SPI buses at once: SPI3 (`SPIClass(HSPI)`) and SPI2 (the default `SPI`, FSPI).
//   HSPI: microSD module through the core's SD library, SCK=18, MISO=16, MOSI=17, CS=15;
//   FSPI: ILI9341 TFT (Adafruit ILI9341), SCK=12, MOSI=11, MISO=13, CS=10, DC=9, RESET=8, LED=7.
// Steps, each printed when done:
//   `tft ready`; SD.begin(15, hspi) retried every second with `mount failed`, then `card type=<..>`;
//   `step=blue` fillScreen blue; `/spi3.txt` written over HSPI one line at a time (`line <i>`,
//   flushed) with a red 20x10 rect at (10, 10 + 20 i) drawn on the TFT after each line, i = 0..3;
//   `read <line>` for each line read back; `done`.
#include <SPI.h>
#include <SD.h>
#include <Adafruit_GFX.h>
#include <Adafruit_ILI9341.h>

#define HSPI_SCK 18
#define HSPI_MISO 16
#define HSPI_MOSI 17
#define SD_CS 15
#define TFT_SCK 12
#define TFT_MISO 13
#define TFT_MOSI 11
#define TFT_CS 10
#define TFT_DC 9
#define TFT_RST 8
#define TFT_LED 7

SPIClass hspi(HSPI);
Adafruit_ILI9341 tft(&SPI, TFT_DC, TFT_CS, TFT_RST);

const char *typeName(uint8_t t) {
  switch (t) {
    case CARD_MMC: return "MMC";
    case CARD_SD: return "SDSC";
    case CARD_SDHC: return "SDHC";
    default: return "UNKNOWN";
  }
}

void setup() {
  Serial.begin(115200);
  pinMode(TFT_LED, OUTPUT);
  digitalWrite(TFT_LED, HIGH);
  SPI.begin(TFT_SCK, TFT_MISO, TFT_MOSI);
  hspi.begin(HSPI_SCK, HSPI_MISO, HSPI_MOSI, SD_CS);
  tft.begin();
  Serial.println("tft ready");
  while (!SD.begin(SD_CS, hspi)) {
    Serial.println("mount failed");
    delay(1000);
  }
  Serial.printf("card type=%s\n", typeName(SD.cardType()));

  tft.fillScreen(ILI9341_BLUE);
  Serial.println("step=blue");

  File f = SD.open("/spi3.txt", FILE_WRITE);
  for (int i = 0; i < 4; i++) {
    f.printf("line %d\n", i);
    f.flush();
    tft.fillRect(10, 10 + 20 * i, 20, 10, ILI9341_RED);
  }
  f.close();

  f = SD.open("/spi3.txt");
  while (f.available()) Serial.printf("read %s\n", f.readStringUntil('\n').c_str());
  f.close();
  Serial.println("done");
}

void loop() { delay(1000); }
