/* Minimal libavif for gamescope. Full libavif pulls rav1e/svt/abseil;
 * gamescope only needs these symbols for optional AVIF encode. */
#include <stdint.h>
void *avifEncoderCreate(void) { return 0; }
void avifEncoderDestroy(void *p) { (void)p; }
int avifEncoderAddImage(void *a, void *b, uint64_t c, int d)
{
	(void)a; (void)b; (void)c; (void)d;
	return 1;
}
int avifEncoderFinish(void *a, void *b)
{
	(void)a; (void)b;
	return 1;
}
void *avifImageCreate(uint32_t w, uint32_t h, uint32_t d, int f)
{
	(void)w; (void)h; (void)d; (void)f;
	return 0;
}
void avifImageDestroy(void *p) { (void)p; }
int avifImageRGBToYUV(void *a, void *b)
{
	(void)a; (void)b;
	return 1;
}
void avifRGBImageSetDefaults(void *a, void *b) { (void)a; (void)b; }
void avifRWDataFree(void *p) { (void)p; }
