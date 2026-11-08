"""生成 Tauri 需要的应用图标（没用外部素材，纯代码画一个）。"""
from PIL import Image, ImageDraw

SIZE = 1024
BG = (11, 13, 16, 255)
ACCENT = (77, 212, 172, 255)
ACCENT_DIM = (52, 160, 132, 255)

img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)

# 圆角底
draw.rounded_rectangle((48, 48, SIZE - 48, SIZE - 48), radius=220, fill=BG)
# 外圈
draw.ellipse((200, 200, SIZE - 200, SIZE - 200), fill=(20, 26, 30, 255))
draw.ellipse((240, 240, SIZE - 240, SIZE - 240), fill=BG)

# 三根均衡器柱（中间高、两边低）
bar_w = 84
gap = 56
heights = [260, 420, 300]
total = len(heights) * bar_w + (len(heights) - 1) * gap
x = (SIZE - total) // 2
base_y = SIZE // 2 + 230
for i, h in enumerate(heights):
    color = ACCENT if i == 1 else ACCENT_DIM
    draw.rounded_rectangle((x, base_y - h, x + bar_w, base_y), radius=bar_w // 2, fill=color)
    x += bar_w + gap

icons_dir = r"E:\Manbamusic\apps\desktop\src-tauri\icons"
img.save(f"{icons_dir}\\icon.png")
img.resize((512, 512), Image.LANCZOS).save(f"{icons_dir}\\icon-512.png")
img.resize((128, 128), Image.LANCZOS).save(f"{icons_dir}\\128x128.png")
img.resize((32, 32), Image.LANCZOS).save(f"{icons_dir}\\32x32.png")
img.save(
    f"{icons_dir}\\icon.ico",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
print("icons written")
