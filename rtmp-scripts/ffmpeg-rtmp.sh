# broadcast rtmp with ffmpeg, dont transcode
ffmpeg -re -stream_loop -1 -i ./source.mp4 -c copy -f flv $1