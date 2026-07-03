import yt_dlp
import sys


def get_channel_video_ids(channel_url):
    ydl_opts = {
        "quiet": True,
        "extract_flat": True,
        "skip_download": True,
    }

    with yt_dlp.YoutubeDL(ydl_opts) as ydl:
        info = ydl.extract_info(channel_url, download=False)

        video_ids = []

        for entry in info.get("entries", []):
            if entry and "id" in entry:
                video_ids.append(entry["id"])

        return video_ids


if __name__ == "__main__":
    channel_url = ""

    channels = ["sml_reuploaded", "SMLMovies", "SMLVideos"]

    ids = []

    for channel in channels:
        foundIds = get_channel_video_ids(f"https://www.youtube.com/@{channel}/videos")
        print(f"found {len(foundIds)} videos from {channel}", file=sys.stderr)
        ids.extend(foundIds)

    print(f"Found {len(ids)} videos\n", file=sys.stderr)
    for vid in ids:
        print(vid)

    with open(".txt", "w", encoding="utf-8") as file:
        for id in ids:
            print(id, file=file)
