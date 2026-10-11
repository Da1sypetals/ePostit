#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <unistd.h>

// CGVirtualDisplay 在当前 SDK 头文件中缺失,运行时类仍然存在,
// 以协议声明签名、NSClassFromString 实例化。
// 创建虚拟显示器并常驻进程:窗口可以移到这块不可见的屏幕上,
// CGWindowList 仍报告 onscreen=1,用户物理屏幕无任何画面。
// 进程退出时虚拟显示器自动销毁。
// 输出: <displayID> <originX> <originY> <width> <height>

@protocol VDDesc
@property(nonatomic, copy) NSString *name;
@property(nonatomic, assign) NSInteger maxPixelsWide;
@property(nonatomic, assign) NSInteger maxPixelsHigh;
@property(nonatomic, assign) CGSize sizeInMillimeters;
@property(nonatomic, assign) unsigned int productID;
@property(nonatomic, assign) unsigned int vendorID;
@property(nonatomic, assign) unsigned int serialNum;
@end

@protocol VDMode
- (instancetype)initWithWidth:(NSInteger)width
                       height:(NSInteger)height
                  refreshRate:(double)refreshRate;
@end

@protocol VDSettings
@property(nonatomic, copy) NSArray *modes;
@end

@protocol VDisplay
- (instancetype)initWithDescriptor:(id)desc;
- (BOOL)applySettings:(id)settings;
@property(nonatomic, readonly) CGDirectDisplayID displayID;
@end

int main(void) {
    @autoreleasepool {
        Class descClass = NSClassFromString(@"CGVirtualDisplayDescriptor");
        Class modeClass = NSClassFromString(@"CGVirtualDisplayMode");
        Class settingsClass = NSClassFromString(@"CGVirtualDisplaySettings");
        Class displayClass = NSClassFromString(@"CGVirtualDisplay");
        if (!descClass || !modeClass || !settingsClass || !displayClass) {
            fputs("classes missing\n", stderr);
            return 1;
        }

        id<VDDesc> desc = [[descClass alloc] init];
        desc.name = @"ePostit-e2e";
        desc.maxPixelsWide = 1920;
        desc.maxPixelsHigh = 1080;
        desc.sizeInMillimeters = CGSizeMake(600, 340);
        desc.vendorID = 0xEEEE;
        desc.productID = 0xE2E2;
        desc.serialNum = 1;

        id<VDisplay> vd = [[displayClass alloc] initWithDescriptor:desc];
        if (!vd) {
            fputs("create failed\n", stderr);
            return 1;
        }
        id<VDSettings> settings = [[settingsClass alloc] init];
        settings.modes = @[ [[modeClass alloc] initWithWidth:1280
                                                    height:720
                                              refreshRate:60] ];
        if (![vd applySettings:settings]) {
            fputs("apply failed\n", stderr);
            return 1;
        }
        // 摆到主屏上方居中(整体 y<0):不与任何物理屏的
        // 下边缘、左边缘、右边缘相邻,Dock 不会迁过去
        CGRect main = CGDisplayBounds(CGMainDisplayID());
        int x = (int)(main.origin.x + (main.size.width - 1280) / 2);
        int y = (int)(main.origin.y - 720 - 40);
        for (int i = 0; i < 20; i++) {
            CGDisplayConfigRef cfg;
            if (CGBeginDisplayConfiguration(&cfg) != kCGErrorSuccess) {
                break;
            }
            CGConfigureDisplayOrigin(cfg, vd.displayID, x, y);
            CGError err = CGCompleteDisplayConfiguration(cfg, kCGConfigureForSession);
            if (err == kCGErrorSuccess) {
                break;
            }
            usleep(50 * 1000);
        }
        CGRect b = CGDisplayBounds(vd.displayID);
        CGRect m = CGDisplayBounds(CGMainDisplayID());
        printf("%u %d %d %d %d %d %d %d %d\n", vd.displayID, (int)b.origin.x,
               (int)b.origin.y, (int)b.size.width, (int)b.size.height,
               (int)m.origin.x, (int)m.origin.y, (int)m.size.width,
               (int)m.size.height);
        fflush(stdout);
        CFRunLoopRun();
    }
    return 0;
}
