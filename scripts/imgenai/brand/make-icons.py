"""生成 imgenai Code 的临时应用图标：官网 favicon 的紫色渐变六边形，放在白色圆角方形底上。

输出到本目录：icon-macos.png（1024，按 macOS 图标网格四周留边）、icon.png（512，铺满）、
icon.ico（Windows，最大尺寸 256 排第一个，见 tests/app_icon.rs）。正式图标定下来后直接替换这三个文件。
用法：python3 make-icons.py
"""
import io
import os
import struct

from PIL import Image, ImageChops, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
SS = 4  # 超采样倍数，画大再缩小来抗锯齿
TOP, BOTTOM = (99, 102, 241), (139, 92, 246)  # #6366f1 → #8b5cf6


def gradient(size):
    """左上到右下的对角渐变。"""
    base = Image.new("RGB", (size, size))
    pixels = base.load()
    for y in range(size):
        for x in range(size):
            t = (x + y) / (2 * (size - 1))
            pixels[x, y] = tuple(round(a + (b - a) * t) for a, b in zip(TOP, BOTTOM))
    return base


def render(canvas, inset, radius):
    """canvas：输出边长；inset：圆角方形距边缘的留白；radius：圆角半径（都按输出像素）。"""
    size = canvas * SS
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    left, right = inset * SS, (canvas - inset) * SS
    # 白色到浅紫的竖向底色
    plate = Image.new("RGBA", (size, size))
    plate_draw = ImageDraw.Draw(plate)
    for y in range(size):
        t = y / (size - 1)
        plate_draw.line([(0, y), (size, y)], fill=(round(255 - 12 * t), round(255 - 13 * t), 255, 255))
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle([left, left, right, right], radius=radius * SS, fill=255)
    image.paste(plate, (0, 0), mask)

    # 六边形：官网 SVG 的 viewBox 0..100，按方形内容区的 62% 缩放居中
    box = right - left
    scale = box * 0.62 / 90
    center = size / 2
    to_px = lambda x, y: (center + (x - 50) * scale, center + (y - 50) * scale)
    hexagon = [to_px(*point) for point in [(50, 5), (95, 27.5), (95, 72.5), (50, 95), (5, 72.5), (5, 27.5)]]
    hex_mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(hex_mask).polygon(hexagon, fill=255)
    fill = gradient(256).resize((size, size)).convert("RGBA")
    image.paste(fill, (0, 0), hex_mask)

    # 白色圆点（主圆 0.9 透明度，三个小圆 0.6）
    dots = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    dots_draw = ImageDraw.Draw(dots)
    for (cx, cy, r, alpha) in [(50, 50, 15, 0.9), (35, 35, 8, 0.6), (65, 35, 8, 0.6), (50, 70, 8, 0.6)]:
        x, y = to_px(cx, cy)
        rr = r * scale
        dots_draw.ellipse([x - rr, y - rr, x + rr, y + rr], fill=(255, 255, 255, round(255 * alpha)))
    image = Image.alpha_composite(image, dots)
    return image.resize((canvas, canvas), Image.LANCZOS)


def write_ico(path, image):
    """多尺寸 ICO，每个条目内嵌 PNG；最大的 256 放第一个。"""
    sizes = [256, 128, 64, 48, 32, 16]
    blobs = []
    for edge in sizes:
        buffer = io.BytesIO()
        image.resize((edge, edge), Image.LANCZOS).save(buffer, format="PNG")
        blobs.append(buffer.getvalue())
    header = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    entries = b""
    for edge, blob in zip(sizes, blobs):
        entries += struct.pack("<BBBBHHII", 0 if edge == 256 else edge, 0 if edge == 256 else edge, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    with open(path, "wb") as handle:
        handle.write(header + entries + b"".join(blobs))


if __name__ == "__main__":
    # macOS 图标网格：1024 画布里内容区 824、四周留 100、圆角约 185。
    render(1024, 100, 185).save(os.path.join(HERE, "icon-macos.png"))
    full = render(1024, 0, 225)
    full.resize((512, 512), Image.LANCZOS).save(os.path.join(HERE, "icon.png"))
    write_ico(os.path.join(HERE, "icon.ico"), full)
    # 管理器界面左下角与「关于」里的小 logo（原版 300×300）。
    full.resize((300, 300), Image.LANCZOS).save(os.path.join(HERE, "app-logo.png"))
    print("已生成 icon-macos.png、icon.png、icon.ico、app-logo.png")
